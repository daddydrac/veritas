from __future__ import annotations

import json
from pathlib import Path

from rdflib import Graph, Namespace
from rdflib.namespace import RDF

from veritas_ingest.chunking import make_chunks
from veritas_ingest.formulas import extract_docling_formula_candidates
from veritas_ingest.human_review import apply_formula_decision, review_formulas_noninteractive
from veritas_ingest.latex_ocr import normalize_latex, ocr_formula_image
from veritas_ingest.sinks import chunks_to_turtle


def test_docling_visual_formula_candidates_are_merged_into_formula_chunks() -> None:
    docling = {
        "texts": [
            {
                "label": "formula",
                "text": "L(\\theta)=\\mathbb{E}_{q_\\theta(z)}[\\log p(x,z)-\\log q_\\theta(z)]",
                "prov": [{"page": 4, "bbox": {"l": 10, "t": 20, "r": 200, "b": 60}}],
            }
        ]
    }
    candidates = extract_docling_formula_candidates(docling)
    chunks = make_chunks("paper", "The method optimizes a variational objective. The rest is prose.", {"title": "t", "visual_formula_candidates": candidates})
    formula_chunks = [c for c in chunks if c["chunk_type"] == "formula"]
    assert formula_chunks
    formula = formula_chunks[0]["formulas"][0]
    assert formula["source"] == "docling_visual"
    assert formula["page"] == 4
    assert formula["bbox"] == [10.0, 20.0, 200.0, 60.0]


def test_latex_ocr_heuristic_preserves_existing_latex() -> None:
    result = ocr_formula_image(Path("missing.png"), existing_latex=" x  =  y ^ 2 ", provider="heuristic")
    assert result.status == "skipped_existing_latex"
    assert result.latex == "x=y^2"
    assert normalize_latex(" x  =  y ^ 2 ") == "x=y^2"


def test_human_formula_review_updates_validation_status(tmp_path: Path) -> None:
    chunks_path = tmp_path / "chunks.jsonl"
    chunk = {
        "chunk_id": "p::formula_chunk::00001",
        "paper_id": "p",
        "ordinal": 1,
        "text": "Formula: E=mc^2",
        "formulas": [{"formula_id": "f1", "latex": "E=mc^2", "human_validated": False}],
        "metadata": {},
    }
    chunks_path.write_text(json.dumps(chunk), encoding="utf-8")
    result = review_formulas_noninteractive(chunks_path, "approve", reviewer="test")
    assert result["formulas_reviewed"] == 1
    updated = json.loads(chunks_path.read_text())
    formula = updated["formulas"][0]
    assert formula["human_validated"] is True
    assert formula["human_validation_status"] == "approve"


def test_rejected_formula_is_blocked_from_codegen() -> None:
    formula = {"formula_id": "f", "latex": "x=y"}
    apply_formula_decision(formula, "reject")
    assert formula["human_validated"] is False
    assert formula["use_for_codegen"] is False


def test_turtle_contains_pass4_formula_metadata() -> None:
    chunks = [
        {
            "chunk_id": "p::formula_chunk::00001",
            "paper_id": "p",
            "ordinal": 1,
            "text": "Formula: E=mc^2",
            "metadata": {"title": "Paper", "pdf_sha256": "abc"},
            "formulas": [
                {
                    "formula_id": "f1",
                    "latex": "E=mc^2",
                    "normalized_latex": "E=mc^2",
                    "source": "docling_visual",
                    "confidence": 0.91,
                    "formula_image_path": "data/formulas/p/f1.png",
                    "formula_image_status": "rendered",
                    "latex_ocr_status": "ocr_complete",
                    "latex_ocr_engine": "http",
                    "human_validation_status": "approve",
                }
            ],
        }
    ]
    ttl = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:test")
    g = Graph().parse(data=ttl, format="turtle")
    V = Namespace("https://github.com/daddydrac/veritas/ontology#")
    formulas = list(g.subjects(RDF.type, V.SymbolicShadow))
    assert formulas
    f = formulas[0]
    assert (f, V.hasFormulaImageStatus, None) in g
    assert (f, V.hasLatexOcrStatus, None) in g
    assert (f, V.hasHumanValidationStatus, None) in g
