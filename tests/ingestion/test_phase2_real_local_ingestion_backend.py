from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def test_real_local_ingestion_backend_writes_required_artifacts(tmp_path: Path) -> None:
    workspace = tmp_path / "journey-local-ingestion"
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
            "phase2_sample",
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
    for artifact in [
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
    ]:
        assert (workspace / artifact).exists(), artifact
    manifest = json.loads((workspace / "evidence_manifest.json").read_text(encoding="utf-8"))
    assert manifest["backend"] == "local"
    assert manifest["chunk_count"] > 0
    assert manifest["citation_count"] >= 1
    assert manifest["embedding"]["status"] == "unavailable"
    assert manifest["planning_status"] == "blocked_retrieval_unavailable"
    assert manifest["planning_blocked"] is True
    assert "embedding" in manifest["embedding"]["remediation"].lower()
    # No fake vectors should be produced when no real provider is configured.
    assert (workspace / "local_vector_index.jsonl").read_text(encoding="utf-8") == ""


def test_real_local_ingestion_backend_is_wired_into_journey_product_path() -> None:
    journey = (ROOT / "apps" / "api" / "src" / "journey.rs").read_text(encoding="utf-8")
    assert "run_local_ingestion" in journey
    assert "veritas_ingest.cli" in journey
    assert "--backend" in journey and "local" in journey
    assert "blocked_by_retrieval_unavailable" in journey
    assert "promote_local_ingestion_artifacts" in journey
    assert "execute_autonomous_run_core" in journey


def test_api_image_contains_real_ingestion_backend() -> None:
    dockerfile = (ROOT / "Dockerfile.api").read_text(encoding="utf-8")
    compose = (ROOT / "docker-compose.yml").read_text(encoding="utf-8")
    assert "services/ingestion/requirements.txt" in dockerfile
    assert "veritas_ingest" in dockerfile
    assert "VERITAS_INGEST_PYTHON" in dockerfile
    assert "./config:/workspace/config:ro" in compose
    assert "./packages/ontology:/workspace/ontology:ro" in compose
