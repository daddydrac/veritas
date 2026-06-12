from __future__ import annotations

import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_phase3_rust_execution_safety_contract_present() -> None:
    main = read("apps/api/src/main.rs")
    required = [
        "effective_command_runner",
        "active_veritas_profile",
        "is_production_profile",
        "VERITAS_ALLOW_LOCAL_COMMAND_RUNNER",
        "local_blocked",
        "command_rejection_reason",
        "--network",
        "none",
        "--pids-limit",
        "--cap-drop",
        "--security-opt",
        "no-new-privileges",
        "--read-only",
        "--tmpfs",
        "safe_output_path",
        "validate_relative_output_path",
        "reject_existing_symlink",
        "verify_existing_path_inside_workspace",
        "run_index.jsonl",
        "command_audit_tail",
    ]
    missing = [needle for needle in required if needle not in main]
    assert not missing


def test_phase3_source_mocked_execution_safety_script_passes() -> None:
    script = ROOT / "scripts/e2e/source-mocked-execution-safety.sh"
    assert os.access(script, os.X_OK)
    result = subprocess.run([str(script)], cwd=ROOT, text=True, capture_output=True, timeout=60, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["ok"] is True
    names = {item["name"] for item in payload["checks"]}
    expected = {
        "safe_write_inside_workspace",
        "unsafe_path_rejected:../evil.py",
        "unsafe_path_rejected:/tmp/evil.py",
        "unsafe_path_rejected:src/../../evil.py",
        "symlink_parent_rejected",
        "command_allowlist_allows_safe",
        "command_allowlist_rejects_dangerous",
        "production_profile_defaults_to_sandbox",
        "production_local_requires_explicit_override",
        "duplicate_lock_rejected",
        "stale_lock_replaced",
        "state_sequence_persisted",
        "run_index_persisted",
        "resume_tools_pending",
        "resume_validation_pending",
        "resume_cancelled_blocked",
    }
    assert expected <= names
