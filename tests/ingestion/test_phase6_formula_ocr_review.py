from __future__ import annotations

import json
import os
import pathlib
import subprocess

from rdflib import Graph

from veritas_ingest.formula_ocr_review_contracts import (
    chunking_edge_contract,
    command_ocr_contract,
    formula_image_contract,
    http_ocr_contract,
    opensearch_mapping_contract,
    review_contract,
    source_mocked_phase6_summary,
)
from veritas_ingest.formula_images import attach_formula_images
from veritas_ingest.human_review import review_citations_in_chunks, review_formulas_noninteractive
from veritas_ingest.latex_ocr import ocr_formula_image
from veritas_ingest.sinks import chunks_to_turtle

ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_phase6_command_ocr_provider_contract(tmp_path: pathlib.Path) -> None:
    result = command_ocr_contract(tmp_path)
    assert result["ok"] is True, result
    assert result["latex"] == "E=mc^2"
    assert result["engine"] == "command"


def test_phase6_http_ocr_provider_contract(tmp_path: pathlib.Path) -> None:
    result = http_ocr_contract(tmp_path)
    assert result["ok"] is True, result
    assert result["engine"] == "http"
    assert "alpha" in result["latex"]


def test_phase6_mock_formula_image_renderer_creates_metadata(tmp_path: pathlib.Path, monkeypatch) -> None:
    monkeypatch.setenv("VERITAS_FORMULA_IMAGE_RENDERER", "mock")
    monkeypatch.setenv("VERITAS_LATEX_OCR_PROVIDER", "heuristic")
    result = formula_image_contract(tmp_path)
    assert result["ok"] is True, result
    formula = result["formula"]
    assert pathlib.Path(formula["formula_image_path"]).exists()
    assert formula["formula_image_status"] == "rendered_mock"
    assert formula["formula_image_engine"] == "mock"
    assert formula["bbox_status"] == "bbox_present"


def test_phase6_formula_image_missing_renderer_is_auditable(tmp_path: pathlib.Path, monkeypatch) -> None:
    monkeypatch.delenv("VERITAS_FORMULA_IMAGE_RENDERER", raising=False)
    monkeypatch.setenv("VERITAS_LATEX_OCR_PROVIDER", "none")
    chunks = [{"paper_id": "p", "chunk_id": "c", "formulas": [{"formula_id": "f", "latex": "x=y"}]}]
    out = attach_formula_images(tmp_path / "missing.pdf", chunks, tmp_path / "formulas")
    formula = out[0]["formulas"][0]
    assert formula["formula_image_status"] in {"not_available_renderer_unavailable", "not_available_no_bbox", "not_available_no_bbox_or_renderer"}
    assert formula["latex_ocr_status"] == "skipped_provider_disabled"
    assert formula["human_validation_status"] == "pending_human_review"


def test_phase6_formula_and_citation_review_persist_to_chunks_and_rdf(tmp_path: pathlib.Path) -> None:
    result = review_contract(tmp_path)
    assert result["ok"] is True, result
    assert result["formula"]["use_for_codegen"] is True
    assert result["metadata"]["citation_review_status"] == "edit"
    assert result["turtle_triples"] > 0


def test_phase6_direct_review_helpers_support_reject_and_incomplete(tmp_path: pathlib.Path) -> None:
    chunks_path = tmp_path / "chunks.jsonl"
    chunks = [{
        "paper_id": "p",
        "chunk_id": "c",
        "ordinal": 0,
        "text": "Formula $$x=y$$.",
        "metadata": {"paper_id": "p", "apa_citation": "Unknown author (n.d.). Untitled."},
        "formulas": [{"formula_id": "f", "latex": "x=y"}],
    }]
    chunks_path.write_text("\n".join(json.dumps(c) for c in chunks), encoding="utf-8")
    formula_result = review_formulas_noninteractive(chunks_path, "reject")
    citation_result = review_citations_in_chunks(chunks_path, "incomplete")
    reviewed = [json.loads(line) for line in chunks_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    assert formula_result["formulas_reviewed"] == 1
    assert citation_result["chunks_updated"] == 1
    assert reviewed[0]["formulas"][0]["use_for_codegen"] is False
    assert reviewed[0]["metadata"]["citation_review_status"] == "incomplete"


def test_phase6_chunking_edge_cases_preserve_multiple_formulas() -> None:
    result = chunking_edge_contract()
    assert result["ok"] is True, result
    assert result["formula_count"] >= 2


def test_phase6_opensearch_mapping_contains_formula_review_fields() -> None:
    result = opensearch_mapping_contract()
    assert result["ok"] is True, result


def test_phase6_source_mocked_script_passes() -> None:
    script = ROOT / "scripts/e2e/source-mocked-formula-ocr-review.sh"
    assert os.access(script, os.X_OK)
    result = subprocess.run([str(script)], cwd=ROOT, text=True, capture_output=True, timeout=60, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["ok"] is True
    names = {check["name"] for check in payload["checks"]}
    assert "command_ocr_provider_returns_expected_latex" in names
    assert "formula_and_citation_review_persist" in names
