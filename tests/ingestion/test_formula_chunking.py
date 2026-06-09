from __future__ import annotations

from rdflib import Graph

from veritas_ingest.chunking import make_chunks
from veritas_ingest.formulas import extract_formulas
from veritas_ingest.sinks import chunks_to_turtle


def test_formula_extraction_skips_obvious_currency() -> None:
    text = "Use $$E=mc^2$$ and $x_i=y^2$ but ignore $20."
    bodies = [formula["latex"] for formula in extract_formulas(text)]
    assert "E=mc^2" in bodies
    assert "x_i=y^2" in bodies
    assert "20" not in bodies


def test_chunks_keep_full_formula() -> None:
    text = "Intro. " + "a" * 40 + " $$" + "x" * 200 + "=1$$ tail."
    chunks = make_chunks(
        "paper-1",
        text,
        {"title": "fixture"},
        target_chars=80,
        overlap_chars=10,
        hard_max_chars=90,
        context_window=5,
    )
    formulas = [formula for chunk in chunks for formula in chunk.get("formulas", [])]
    assert len(formulas) == 1
    assert "=1" in formulas[0]["latex"]


def test_turtle_parses_with_latex_backslashes() -> None:
    text = r"Let $$\\alpha_i = \\beta^2$$."
    chunks = [
        {
            "chunk_id": "paper-1::chunk::00000",
            "paper_id": "paper-1",
            "ordinal": 0,
            "text": text,
            "formulas": extract_formulas(text),
            "metadata": {"title": "fixture", "pdf_sha256": "abc"},
        }
    ]
    turtle = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:test")
    Graph().parse(data=turtle, format="turtle")
