from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SAMPLE = ROOT / "tests" / "fixtures" / "sample_math_paper.pdf"


def run_ingest_cli(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "services" / "ingestion")
    return subprocess.run(
        ["python3", "-m", "veritas_ingest.cli", "--config", str(ROOT / "config" / "veritas.yaml"), *args],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=True,
    )


def load(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def ingest(workspace: Path) -> None:
    run_ingest_cli(
        "ingest-pdf",
        "--path",
        str(SAMPLE),
        "--backend",
        "local",
        "--workspace",
        str(workspace),
        "--paper-id",
        "sample_math_paper",
    )


def test_local_ingestion_writes_authoritative_evidence_registry(tmp_path: Path) -> None:
    workspace = tmp_path / "ingest"
    ingest(workspace)
    registry = load(workspace / "evidence_registry.json")
    eligibility = load(workspace / "evidence_eligibility.json")
    manifest = load(workspace / "evidence_manifest.json")

    assert registry["kind"] == "VeritasEvidenceEligibilityRegistry"
    assert manifest["evidence_registry_path"].endswith("evidence_registry.json")
    assert eligibility["planning"]["allowed"] is False
    assert registry["summary"]["citations_usable_for_audit"] == 0
    assert registry["summary"]["formulas_eligible_for_codegen"] == 0
    assert registry["formulas"][0]["codegen_eligibility_status"] in {
        "pending_review",
        "not_eligible_missing_citation",
        "not_eligible_low_confidence",
    }


def test_reviews_refresh_registry_and_make_formula_eligible_only_after_citation_approval(tmp_path: Path) -> None:
    workspace = tmp_path / "ingest"
    ingest(workspace)

    run_ingest_cli("review-formulas", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve")
    registry_after_formula = load(workspace / "evidence_registry.json")
    assert registry_after_formula["summary"]["formulas_eligible_for_codegen"] == 0
    assert registry_after_formula["formulas"][0]["codegen_eligibility_status"] == "not_eligible_missing_citation"

    run_ingest_cli("review-citations", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve")
    registry = load(workspace / "evidence_registry.json")
    assert registry["summary"]["citations_usable_for_audit"] == 1
    assert registry["summary"]["formulas_eligible_for_codegen"] == 1
    formula_id = registry["formulas"][0]["formula_id"]
    result = run_ingest_cli("evidence-registry", "--workspace", str(workspace), "--formula-id", formula_id)
    payload = json.loads(result.stdout)
    assert payload["formula_gate"]["ok"] is True
    assert payload["formula_gate"]["status"] == "eligible"


def test_rejected_formula_blocks_formula_gate(tmp_path: Path) -> None:
    workspace = tmp_path / "ingest"
    ingest(workspace)
    run_ingest_cli("review-citations", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "approve")
    run_ingest_cli("review-formulas", "--chunks", str(workspace / "chunks.jsonl"), "--decision", "reject")
    registry = load(workspace / "evidence_registry.json")
    formula_id = registry["formulas"][0]["formula_id"]
    result = run_ingest_cli("evidence-registry", "--workspace", str(workspace), "--formula-id", formula_id)
    payload = json.loads(result.stdout)
    assert payload["formula_gate"]["ok"] is False
    assert payload["formula_gate"]["status"] == "blocked_by_formula_review"
    assert registry["formulas"][0]["codegen_eligibility_status"] == "rejected"
