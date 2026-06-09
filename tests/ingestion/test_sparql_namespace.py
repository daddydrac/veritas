from __future__ import annotations

from pathlib import Path

from rdflib import Graph

from veritas_ingest.sinks import chunks_to_turtle


NAMESPACE = "https://github.com/daddydrac/veritas/ontology#"


def _sample_chunks() -> list[dict]:
    return [
        {
            "chunk_id": "paper-x::chunk::00000",
            "paper_id": "paper-x",
            "ordinal": 0,
            "text": "A theorem with $$E = mc^2$$ and surrounding prose.",
            "metadata": {"title": "Synthetic Paper", "pdf_sha256": "abc"},
            "formulas": [
                {
                    "latex": "E = mc^2",
                    "raw_latex": "$$E = mc^2$$",
                    "start": 15,
                    "end": 27,
                    "source": "test",
                    "pattern": "display_dollars",
                    "confidence": 0.9,
                }
            ],
        }
    ]


def test_formula_traceability_query_uses_veritas_ontology_namespace() -> None:
    turtle = chunks_to_turtle(_sample_chunks(), NAMESPACE, "urn:test")
    graph = Graph()
    graph.parse(data=turtle, format="turtle")
    query = Path("packages/ontology/queries/formula_traceability.sparql").read_text()
    rows = list(graph.query(query))
    assert len(rows) == 1
    assert "E = mc^2" in str(rows[0].expr)


def test_evidence_chunks_query_uses_veritas_ontology_namespace() -> None:
    turtle = chunks_to_turtle(_sample_chunks(), NAMESPACE, "urn:test")
    graph = Graph()
    graph.parse(data=turtle, format="turtle")
    query = Path("packages/ontology/queries/evidence_chunks.sparql").read_text()
    rows = list(graph.query(query))
    assert len(rows) == 1
    assert "theorem" in str(rows[0].text)
