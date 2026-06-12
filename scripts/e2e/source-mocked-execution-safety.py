#!/usr/bin/env python3
"""Source/mocked Phase 3 execution-safety proof.

This script does not require Docker, Cargo, or live vLLM. It proves the safety
contract that the Rust API mirrors: generated files cannot escape the run
workspace, production profiles default to sandbox semantics, dangerous commands
are rejected, run locks are atomic/stale-aware, cancellation blocks resume, and
state/events/run-index artifacts are durable.
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import tempfile
import time
from dataclasses import dataclass
from typing import Any

ALLOWED_PREFIXES = (
    "cargo fmt",
    "cargo check",
    "cargo test",
    "cargo clippy",
    "python -m pytest",
    "python3 -m pytest",
    "python -m build",
    "python3 -m build",
    "ruff",
    "mypy",
    "cmake",
    "ctest",
)
DENIED_SUBSTRINGS = (
    ";",
    "&&",
    "||",
    "|",
    "`",
    "$(",
    "\n",
    "\r",
    ">",
    "<",
    "rm ",
    "rm -",
    "sudo",
    "curl ",
    "wget ",
    "mkfs",
    "dd ",
    "chmod ",
    "chown ",
    "docker ",
    "podman ",
    "ssh ",
    "scp ",
    "nc ",
    "bash -c",
    "sh -c",
    "python -c",
    "python3 -c",
)
PRODUCTION_PROFILES = {"production", "prod", "host-prod", "single-gpu-prod", "multi-gpu-prod", "remote-model-prod"}


@dataclass
class SafetyResult:
    name: str
    ok: bool
    details: dict[str, Any]


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def command_rejection_reason(command: str) -> str | None:
    c = command.strip()
    if not c:
        return "empty"
    for token in DENIED_SUBSTRINGS:
        if token in c:
            return f"denied token {token!r}"
    if any(c == prefix or c.startswith(prefix + " ") for prefix in ALLOWED_PREFIXES):
        return None
    return "not allowlisted"


def active_runner(profile: str, configured: str | None = None) -> str:
    if configured:
        return configured.lower()
    return "sandbox" if profile.lower() in PRODUCTION_PROFILES or profile.lower().endswith("-prod") else "local"


def local_allowed(profile: str, explicit_allow: bool = False) -> bool:
    return explicit_allow or not (profile.lower() in PRODUCTION_PROFILES or profile.lower().endswith("-prod"))


def validate_relative_output_path(rel: str) -> list[str]:
    p = pathlib.PurePosixPath(rel)
    if not rel.strip():
        raise ValueError("empty path")
    if p.is_absolute():
        raise ValueError("absolute path")
    parts = [part for part in p.parts if part not in ("", ".")]
    if not parts:
        raise ValueError("no file")
    if ".." in parts:
        raise ValueError("parent path")
    return parts


def safe_write(workspace: pathlib.Path, rel: str, content: str) -> pathlib.Path:
    parts = validate_relative_output_path(rel)
    root = workspace.resolve()
    cursor = root
    for part in parts[:-1]:
        cursor = cursor / part
        if cursor.exists() and cursor.is_symlink():
            raise ValueError(f"symlink parent: {cursor}")
    target = root.joinpath(*parts)
    parent = target.parent
    parent.mkdir(parents=True, exist_ok=True)
    if not str(parent.resolve()).startswith(str(root) + os.sep) and parent.resolve() != root:
        raise ValueError("parent escapes workspace")
    if target.exists() and target.is_symlink():
        raise ValueError("target symlink")
    tmp = target.with_suffix(target.suffix + ".veritas_tmp")
    tmp.write_text(content, encoding="utf-8")
    tmp.replace(target)
    if target.is_symlink():
        raise ValueError("post-write symlink")
    if not str(target.resolve()).startswith(str(root) + os.sep):
        raise ValueError("target escapes workspace")
    return target


def persist_state(workspace: pathlib.Path, state: str, payload: dict[str, Any]) -> None:
    events_path = workspace / "events.jsonl"
    sequence = 0
    if events_path.exists():
        sequence = len(events_path.read_text(encoding="utf-8").splitlines())
    event = {"ts_ms": int(time.time() * 1000), "sequence": sequence, "state": state, "payload": payload}
    (workspace / "state.json").write_text(json.dumps(event, indent=2), encoding="utf-8")
    with events_path.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(event, sort_keys=True) + "\n")
    run_index = workspace.parent / "run_index.jsonl"
    with run_index.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps({"run_id": workspace.name, "workspace": str(workspace), **event}, sort_keys=True) + "\n")


def acquire_lock(workspace: pathlib.Path, stale_after: int = 7200) -> int:
    lock = workspace / "run.lock"
    if lock.exists() and time.time() - lock.stat().st_mtime > stale_after:
        lock.unlink()
    fd = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    os.write(fd, json.dumps({"run_id": workspace.name, "pid": os.getpid(), "created_at_ms": int(time.time() * 1000)}).encode() + b"\n")
    return fd


def release_lock(fd: int, workspace: pathlib.Path) -> None:
    os.close(fd)
    lock = workspace / "run.lock"
    if lock.exists():
        lock.unlink()


def resume_decision(workspace: pathlib.Path) -> str:
    if (workspace / "CANCELLED").exists():
        return "blocked_cancelled"
    if (workspace / "final_report.json").exists():
        return "already_final"
    if (workspace / "code_package_latest.json").exists() and not (workspace / "validation_results.json").exists():
        return "validation_pending"
    if (workspace / "validation_results.json").exists() and not (workspace / "final_report.json").exists():
        return "repair_or_finalization_pending"
    if (workspace / "plan_envelope.json").exists() and not (workspace / "tool_outputs.json").exists():
        return "tools_pending"
    return "planning_pending"


def run() -> dict[str, Any]:
    root = pathlib.Path(tempfile.mkdtemp(prefix="veritas-phase3-"))
    try:
        runs_dir = root / "runs"
        workspace = runs_dir / "run-phase3"
        workspace.mkdir(parents=True)
        outside = root / "outside"
        outside.mkdir()

        checks: list[SafetyResult] = []

        good = safe_write(workspace, "src/lib.py", "def ok(): return True\n")
        checks.append(SafetyResult("safe_write_inside_workspace", good.exists(), {"path": str(good)}))

        for rel in ["../evil.py", "/tmp/evil.py", "src/../../evil.py"]:
            try:
                safe_write(workspace, rel, "bad")
                checks.append(SafetyResult(f"unsafe_path_rejected:{rel}", False, {"error": "accepted"}))
            except ValueError as exc:
                checks.append(SafetyResult(f"unsafe_path_rejected:{rel}", True, {"reason": str(exc)}))

        link = workspace / "link_out"
        link.symlink_to(outside, target_is_directory=True)
        try:
            safe_write(workspace, "link_out/evil.py", "bad")
            checks.append(SafetyResult("symlink_parent_rejected", False, {"error": "accepted"}))
        except ValueError as exc:
            checks.append(SafetyResult("symlink_parent_rejected", True, {"reason": str(exc)}))

        allowed = ["cargo check", "cargo test --all", "python -m pytest -q", "ruff check .", "ctest --output-on-failure"]
        rejected = ["curl http://example.com", "python -c 'print(1)'", "python -m pytest -q; rm -rf /", "docker run alpine", "sudo true"]
        checks.append(SafetyResult("command_allowlist_allows_safe", all(command_rejection_reason(c) is None for c in allowed), {"allowed": allowed}))
        checks.append(SafetyResult("command_allowlist_rejects_dangerous", all(command_rejection_reason(c) is not None for c in rejected), {"rejected": rejected}))
        checks.append(SafetyResult("production_profile_defaults_to_sandbox", active_runner("single-gpu-prod") == "sandbox", {"runner": active_runner("single-gpu-prod")}))
        checks.append(SafetyResult("production_local_requires_explicit_override", not local_allowed("single-gpu-prod") and local_allowed("single-gpu-prod", True), {}))

        fd = acquire_lock(workspace)
        try:
            try:
                second = acquire_lock(workspace)
                release_lock(second, workspace)
                checks.append(SafetyResult("duplicate_lock_rejected", False, {"error": "second lock acquired"}))
            except FileExistsError:
                checks.append(SafetyResult("duplicate_lock_rejected", True, {}))
        finally:
            release_lock(fd, workspace)

        stale = workspace / "run.lock"
        stale.write_text("stale", encoding="utf-8")
        old = time.time() - 9999
        os.utime(stale, (old, old))
        fd = acquire_lock(workspace, stale_after=1)
        release_lock(fd, workspace)
        checks.append(SafetyResult("stale_lock_replaced", not stale.exists(), {}))

        persist_state(workspace, "Created", {"goal": "phase3"})
        persist_state(workspace, "Planned", {"plan": "plan_envelope.json"})
        persist_state(workspace, "GeneratingCode", {"attempt": 0})
        events = (workspace / "events.jsonl").read_text(encoding="utf-8").splitlines()
        index_events = (runs_dir / "run_index.jsonl").read_text(encoding="utf-8").splitlines()
        checks.append(SafetyResult("state_sequence_persisted", [json.loads(e)["sequence"] for e in events] == [0, 1, 2], {}))
        checks.append(SafetyResult("run_index_persisted", len(index_events) == 3 and all("run-phase3" in line for line in index_events), {}))

        (workspace / "plan_envelope.json").write_text("{}", encoding="utf-8")
        checks.append(SafetyResult("resume_tools_pending", resume_decision(workspace) == "tools_pending", {"decision": resume_decision(workspace)}))
        (workspace / "tool_outputs.json").write_text("[]", encoding="utf-8")
        (workspace / "code_package_latest.json").write_text("{}", encoding="utf-8")
        checks.append(SafetyResult("resume_validation_pending", resume_decision(workspace) == "validation_pending", {"decision": resume_decision(workspace)}))
        (workspace / "CANCELLED").write_text("cancelled", encoding="utf-8")
        checks.append(SafetyResult("resume_cancelled_blocked", resume_decision(workspace) == "blocked_cancelled", {"decision": resume_decision(workspace)}))

        payload = {"ok": all(item.ok for item in checks), "checks": [item.__dict__ for item in checks], "workspace": str(workspace)}
        print(json.dumps(payload, indent=2, sort_keys=True))
        if not payload["ok"]:
            raise SystemExit(1)
        return payload
    finally:
        if os.environ.get("VERITAS_KEEP_PHASE3_WORKSPACE", "false").lower() != "true":
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    run()
