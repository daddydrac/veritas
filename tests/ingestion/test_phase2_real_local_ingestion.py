from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def run_ingest(tmp_path: Path) -> tuple[dict, Path]:
    output_dir = tmp_path / "local_ingestion"
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "services" / "ingestion")
    env["VERITAS_LOCAL_EMBEDDING_PROVIDER"] = "none"
    cmd = [
        sys.executable,
        "-m",
        "veritas_ingest.cli",
        "--config",
        str(ROOT / "config" / "veritas.yaml"),
        "ingest-pdf",
        "--path",
        str(ROOT / "tests" / "fixtures" / "sample_math_paper.pdf"),
        "--backend",
        "local",
        "--workspace",
        str(output_dir),
    ]
    completed = subprocess.run(cmd, cwd=ROOT, env=env, check=True, capture_output=True, text=True)
    payload = None
    for index in [i for i, ch in enumerate(completed.stdout) if ch == "{"][::-1]:
        try:
            payload = json.loads(completed.stdout[index:])
            break
        except json.JSONDecodeError:
            continue
    assert payload is not None, completed.stdout
    return payload, output_dir


def test_local_ingestion_writes_real_manifests_without_services(tmp_path: Path) -> None:
    payload, output_dir = run_ingest(tmp_path)
    assert payload["ok"] is True
    assert payload["output"]["backend"] == "local"
    expected = [
        "chunks.jsonl",
        "formulas.jsonl",
        "citations.jsonl",
        "evidence.ttl",
        "local_lexical_index.jsonl",
        "local_vector_index.jsonl",
        "evidence_manifest.json",
        "formula_manifest.json",
        "citation_manifest.json",
        "review_queue.json",
        "ingestion_report.md",
    ]
    for name in expected:
        assert (output_dir / name).exists(), name
    manifest = json.loads((output_dir / "evidence_manifest.json").read_text())
    assert manifest["backend"] == "local"
    assert manifest["chunk_count"] > 0
    assert manifest["formula_count"] > 0
    assert manifest["citation_count"] == 1
    assert manifest["lexical_index"]["status"] == "available"
    assert manifest["embedding"]["status"] == "unavailable"
    assert manifest["planning_blocked"] is True
    assert "Install" in " ".join(manifest["next_actions"])


def test_local_ingestion_review_queue_is_user_visible(tmp_path: Path) -> None:
    _payload, output_dir = run_ingest(tmp_path)
    queue = json.loads((output_dir / "review_queue.json").read_text())
    assert queue["kind"] == "VeritasReviewQueue"
    assert queue["formula_review"]
    assert queue["citation_review"]
    first_formula = queue["formula_review"][0]
    assert first_formula["formula_id"]
    assert first_formula["latex"]
    assert "recommended_action" in first_formula
    first_citation = queue["citation_review"][0]
    assert first_citation["apa_citation"]
    assert "recommended_action" in first_citation
