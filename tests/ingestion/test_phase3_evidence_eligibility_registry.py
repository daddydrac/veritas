from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run_ingest(workspace: Path, paper_id: str = "phase3_sample") -> None:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "services" / "ingestion")
    env["VERITAS_LOCAL_EMBEDDING_PROVIDER"] = "none"
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "veritas_ingest.cli",
            "--config",
            str(ROOT / "config" / "veritas.yaml"),
            "ingest-pdf",
            "--path",
            str(ROOT / "tests" / "fixtures" / "sample_math_paper.pdf"),
            "--paper-id",
            paper_id,
            "--backend",
            "local",
            "--workspace",
            str(workspace),
        ],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
    )
    assert result.returncode == 0, result.stderr


def run_cli(args: list[str], *, env: dict[str, str] | None = None) -> dict:
    merged = os.environ.copy()
    merged["PYTHONPATH"] = str(ROOT / "services" / "ingestion")
    if env:
        merged.update(env)
    result = subprocess.run(
        [sys.executable, "-m", "veritas_ingest.cli", *args],
        cwd=ROOT,
        env=merged,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
    )
    assert result.returncode == 0, result.stderr
    lines = [line for line in result.stdout.splitlines() if line.strip().startswith("{")]
    return json.loads("\n".join(result.stdout.splitlines()[result.stdout.splitlines().index(lines[0]):])) if lines else json.loads(result.stdout)


def test_local_ingestion_writes_authoritative_evidence_registry(tmp_path: Path) -> None:
    workspace = tmp_path / "registry"
    run_ingest(workspace)
    registry = json.loads((workspace / "evidence_registry.json").read_text(encoding="utf-8"))
    assert registry["kind"] == "VeritasEvidenceEligibilityRegistry"
    assert registry["summary"]["citations_total"] >= 1
    assert registry["summary"]["formulas_total"] >= 1
    assert registry["planning"]["allowed"] is False
    assert "approved" in " ".join(registry["planning"]["blocking_reasons"]).lower() or "review" in " ".join(registry["planning"]["blocking_reasons"]).lower()
    assert (workspace / "evidence_eligibility.json").exists()


def test_review_decisions_refresh_registry_and_control_formula_gate(tmp_path: Path) -> None:
    workspace = tmp_path / "registry-review"
    run_ingest(workspace, paper_id="phase3_review")
    cite = run_cli(["review-citations", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve", "--reviewer", "test"])
    assert cite["evidence_registry"]["summary"]["citations_usable_for_audit"] == 1
    formula = run_cli(["review-formulas", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve", "--reviewer", "test"])
    assert formula["evidence_registry"]["summary"]["formulas_eligible_for_codegen"] >= 1
    registry = json.loads((workspace / "evidence_registry.json").read_text(encoding="utf-8"))
    formula_id = registry["eligible_formulas"][0]
    gate = run_cli(["evidence-registry", "--workspace", str(workspace), "--formula-id", formula_id])
    assert gate["formula_gate"]["ok"] is True
    assert gate["formula_gate"]["status"] == "eligible"


def test_rejected_formula_blocks_formula_gate(tmp_path: Path) -> None:
    workspace = tmp_path / "registry-reject"
    run_ingest(workspace, paper_id="phase3_reject")
    run_cli(["review-citations", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve", "--reviewer", "test"])
    run_cli(["review-formulas", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "reject", "--reviewer", "test"])
    registry = json.loads((workspace / "evidence_registry.json").read_text(encoding="utf-8"))
    blocked = registry["blocked_formulas"][0]["formula_id"]
    gate = run_cli(["evidence-registry", "--workspace", str(workspace), "--formula-id", blocked])
    assert gate["formula_gate"]["ok"] is False
    assert gate["formula_gate"]["status"] == "blocked_by_formula_review"
    assert "rejected" in gate["formula_gate"]["reason"].lower()


def test_api_math_to_code_uses_registry_not_request_boolean() -> None:
    main = (ROOT / "apps" / "api" / "src" / "main.rs").read_text(encoding="utf-8")
    registry = (ROOT / "apps" / "api" / "src" / "evidence_registry.rs").read_text(encoding="utf-8")
    assert "mod evidence_registry;" in main
    assert "require_math_to_code_eligibility" in main
    assert "human_approved" not in main or "request-level approval" in registry
    assert "evidence_registry.formula_not_eligible" in registry
    assert "evidence_registry.citation_not_eligible" in registry
