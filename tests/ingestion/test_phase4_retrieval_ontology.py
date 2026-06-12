from __future__ import annotations

import json
import os
import pathlib
import subprocess

from veritas_ingest.retrieval_ontology_contracts import (
    DEFAULT_QUERY_PACK_NAMES,
    MockOpenSearchTransport,
    assert_vector_dimension,
    build_alias_actions,
    build_opensearch_mapping,
    fixture_sparql_results,
    graph_store_request,
    mapping_contract_violations,
    named_graph_uris,
    retrieval_fallback,
    run_report_to_turtle,
    source_mocked_phase4_summary,
    summarize_sparql_results,
)

ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_phase4_opensearch_mapping_contract_fields() -> None:
    mapping = build_opensearch_mapping("embedding", 768, "v1")
    assert mapping_contract_violations(mapping, "embedding", 768) == []
    props = mapping["mappings"]["properties"]
    assert props["chunk_id"]["type"] == "keyword"
    assert props["text"]["type"] == "text"
    assert props["embedding"]["type"] == "knn_vector"
    assert props["embedding"]["method"]["engine"] == "faiss"
    assert props["embedding"]["method"]["name"] == "hnsw"
    assert props["formulas"]["type"] == "nested"
    assert props["citations"]["type"] == "nested"


def test_phase4_vector_dimension_mismatch_is_rejected() -> None:
    mapping = build_opensearch_mapping("embedding", 768, "v1")
    try:
        assert_vector_dimension(mapping, "embedding", 1024)
    except ValueError as exc:
        assert "dimension mismatch" in str(exc)
    else:
        raise AssertionError("dimension mismatch was not rejected")


def test_phase4_alias_migration_actions_and_idempotency() -> None:
    aliases = {"old-index": {"aliases": {"veritas-evidence-read": {}, "veritas-evidence-write": {"is_write_index": True}}}}
    actions = build_alias_actions(aliases, "veritas-evidence-v1", "veritas-evidence-read", "veritas-evidence-write")
    assert any("remove" in action for action in actions)
    assert any(action.get("add", {}).get("is_write_index") is True for action in actions)

    mock = MockOpenSearchTransport(aliases=aliases)
    mapping = build_opensearch_mapping()
    first = mock.migrate(target_index="veritas-evidence-v1", mapping=mapping, read_alias="veritas-evidence-read", write_alias="veritas-evidence-write")
    second = mock.migrate(target_index="veritas-evidence-v1", mapping=mapping, read_alias="veritas-evidence-read", write_alias="veritas-evidence-write")
    assert first["ok"] is True
    assert any(call.get("status") == "already_exists" for call in second["calls"])


def test_phase4_retrieval_fallback_selects_first_successful_alias() -> None:
    result = retrieval_fallback(
        {
            "veritas-evidence-read": {"ok": False, "error": "missing read alias"},
            "veritas-evidence-write": {"ok": True, "result": {"hits": {"hits": [{"_id": "chunk-1"}]}}},
        },
        ["veritas-evidence-read", "veritas-evidence-write", "veritas-evidence", "veritas-evidence-v1"],
    )
    assert result["ok"] is True
    assert result["target"] == "veritas-evidence-write"
    assert result["attempts"][0]["target"] == "veritas-evidence-read"


def test_phase4_fuseki_named_graphs_and_no_pdf_binary_contract() -> None:
    graphs = named_graph_uris(document_hash="sha256:abc", run_id="run-123")
    assert len(set(graphs.values())) == 4
    assert graphs["ontology"] == "urn:veritas:graph:ontology"
    assert graphs["document"].startswith("urn:veritas:graph:document:")
    turtle = "@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n<urn:x> a veritas:SourceDocument .\n"
    request = graph_store_request(graphs["document"], turtle)
    assert request["method"] == "POST"
    assert request["headers"]["Content-Type"] == "text/turtle"
    try:
        graph_store_request(graphs["document"], "%PDF-1.7 binary")
    except ValueError as exc:
        assert "PDF binary" in str(exc)
    else:
        raise AssertionError("PDF binary payload was not rejected")


def test_phase4_run_report_turtle_has_source_build_validation() -> None:
    ttl = run_report_to_turtle("run-abc", {"final_status": "production_candidate_validated", "files_changed": ["src/lib.rs"], "validation_results": [{"success": True}]})
    assert "SourceCodeArtifact" in ttl
    assert "BuildArtifact" in ttl
    assert "VerificationResult" in ttl
    assert "src/lib.rs" in ttl


def test_phase4_sparql_summary_covers_query_pack() -> None:
    summary = summarize_sparql_results(fixture_sparql_results())
    assert set(DEFAULT_QUERY_PACK_NAMES) <= set(summary["queries"].keys())
    assert all(item["count"] == 1 for item in summary["queries"].values())
    assert summary["warnings"] == []


def test_phase4_source_mocked_script_passes() -> None:
    script = ROOT / "scripts/e2e/source-mocked-retrieval-ontology.sh"
    assert os.access(script, os.X_OK)
    result = subprocess.run([str(script)], cwd=ROOT, text=True, capture_output=True, timeout=60, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["ok"] is True
    names = {check["name"] for check in payload["checks"]}
    expected = {
        "opensearch_mapping_contract",
        "vector_dimension_mismatch_rejected",
        "opensearch_migration_alias_update",
        "opensearch_migration_idempotent_second_run",
        "opensearch_retrieval_fallback_read_to_write",
        "fuseki_named_graphs_distinct",
        "fuseki_graph_store_upload_contract",
        "fuseki_rejects_pdf_binary_payload",
        "run_report_rdf_contains_source_build_validation",
        "planner_sparql_fact_summary_all_queries",
    }
    assert expected <= names


def test_phase4_source_summary_direct_call_passes() -> None:
    payload = source_mocked_phase4_summary()
    assert payload["ok"] is True
    assert payload["summary"]["checks"] >= 10
