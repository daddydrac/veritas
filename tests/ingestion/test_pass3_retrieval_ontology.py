from __future__ import annotations

import json
from pathlib import Path

from rdflib import Graph

from veritas_ingest.formulas import extract_formulas
from veritas_ingest.sinks import chunks_to_turtle, document_graph_uri


def test_document_graph_uri_uses_content_hash_and_document_base() -> None:
    cfg = {"ontology": {"document_graph_base_uri": "urn:veritas:graph:document"}}
    graph_uri = document_graph_uri(cfg, "paper/abc", {"pdf_sha256": "sha256:abc123"})
    assert graph_uri == "urn:veritas:graph:document:sha256_abc123"


def test_document_graph_turtle_contains_source_chunk_formula_and_citation() -> None:
    chunks = [
        {
            "chunk_id": "paper-1::chunk::00000",
            "paper_id": "paper-1",
            "ordinal": 0,
            "text": "Energy relation $$E=mc^2$$.",
            "formulas": extract_formulas("Energy relation $$E=mc^2$$."),
            "embedding_model": "Muennighoff/SBERT-base-nli-v2",
            "embedding_norm": 1.0,
            "metadata": {
                "title": "Fixture Paper",
                "apa_citation": "Doe, J. (2024). Fixture Paper.",
                "authors": ["Doe, J."],
                "year": "2024",
                "pdf_sha256": "abc",
                "pdf_url": "https://arxiv.org/pdf/0000.00000",
            },
        }
    ]
    ttl = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:veritas:graph:document:abc")
    graph = Graph().parse(data=ttl, format="turtle")
    text = ttl
    assert len(graph) > 0
    assert "SourceDocument" in text
    assert "RetrievalResult" in text
    assert "SymbolicShadow" in text
    assert "Doe, J. (2024). Fixture Paper." in text
    assert "E=mc^2" in text


def test_pass3_api_source_contains_aliases_named_graphs_and_query_pack() -> None:
    main = Path("apps/api/src/main.rs").read_text(encoding="utf-8")
    required = [
        "OpenSearchMigrateRequest",
        "opensearch_read_alias",
        "opensearch_write_alias",
        "build_alias_actions",
        "graph_upload",
        "graph_describe",
        "planner_fact_summary",
        "query_pack",
        "upload_run_report_to_fuseki",
        "jena_fuseki_planner_fact_summary",
    ]
    missing = [item for item in required if item not in main]
    assert not missing


def test_pass3_opensearch_schema_file_declares_nested_formulas_and_vectors() -> None:
    # This schema is intentionally a contract-level fixture; the Rust endpoint returns
    # the live mapping, but CI without Rust still verifies the documented contract exists.
    schema_path = Path("schemas/opensearch/evidence_document.schema.json")
    assert schema_path.exists()
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    assert "chunk_id" in schema.get("required", [])
    assert "embedding" in schema["properties"]
    assert "formulas" in schema["properties"]
