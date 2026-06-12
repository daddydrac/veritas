"""Source-level retrieval and ontology hardening contracts for Veritas.

These helpers mirror the Rust API's OpenSearch/Fuseki behavior without
requiring live OpenSearch, Fuseki, Docker, Cargo, or vLLM. They are used by
Phase 4 tests to prove that Veritas owns the schema, migration, named-graph,
SPARQL-summary, and RDF-contract logic at source/mocked level.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable
import json

DEFAULT_QUERY_PACK_NAMES = [
    "formula_traceability",
    "evidence_chunks",
    "formulas_without_invariants",
    "risks_without_mitigation",
    "plans_without_validation",
    "unvalidated_code_artifacts",
    "builds_without_tests",
    "loops_without_termination",
    "objectives_blocked_by_assumptions",
    "deployment_units_without_observability",
    "math_claims_without_transfer_tests",
]


def build_opensearch_mapping(vector_field: str = "embedding", dimension: int = 768, version: str = "v1") -> dict[str, Any]:
    """Build the production OpenSearch FAISS/HNSW mapping contract.

    The shape intentionally mirrors `production_opensearch_mapping` in the Rust
    API. Tests use this function to verify keyword/text/nested/vector contracts
    without live OpenSearch.
    """

    if not vector_field or not vector_field.strip():
        raise ValueError("vector_field must be non-empty")
    if int(dimension) <= 0:
        raise ValueError("dimension must be positive")
    dimension = int(dimension)
    return {
        "settings": {
            "index": {
                "number_of_shards": 1,
                "number_of_replicas": 0,
                "knn": True,
                "knn.algo_param.ef_search": 100,
            }
        },
        "mappings": {
            "_meta": {
                "schema": "veritas_evidence",
                "version": version,
                "vector_field": vector_field,
                "vector_dimension": dimension,
                "owner": "veritas-rust-api",
                "phase4_contract": "source_mocked_retrieval_ontology",
            },
            "properties": {
                "doc_id": {"type": "keyword"},
                "paper_id": {"type": "keyword"},
                "chunk_id": {"type": "keyword"},
                "formula_id": {"type": "keyword"},
                "run_id": {"type": "keyword"},
                "source_type": {"type": "keyword"},
                "status": {"type": "keyword"},
                "sha256": {"type": "keyword"},
                "title": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 512}}},
                "abstract": {"type": "text"},
                "apa_citation": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 1024}}},
                "text": {"type": "text"},
                "chunk_text": {"type": "text"},
                "formula_description": {"type": "text"},
                "technical_summary": {"type": "text"},
                "embedding_model": {"type": "keyword"},
                "embedding_norm": {"type": "float"},
                vector_field: {
                    "type": "knn_vector",
                    "dimension": dimension,
                    "space_type": "cosinesimil",
                    "method": {
                        "name": "hnsw",
                        "engine": "faiss",
                        "parameters": {"ef_construction": 128, "m": 24},
                    },
                },
                "formula_embedding": {
                    "type": "knn_vector",
                    "dimension": dimension,
                    "space_type": "cosinesimil",
                    "method": {
                        "name": "hnsw",
                        "engine": "faiss",
                        "parameters": {"ef_construction": 128, "m": 24},
                    },
                },
                "formulas": {
                    "type": "nested",
                    "properties": {
                        "formula_id": {"type": "keyword"},
                        "latex": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 4096}}},
                        "normalized_latex": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 4096}}},
                        "description": {"type": "text"},
                        "formula_image_path": {"type": "keyword"},
                        "formula_image_status": {"type": "keyword"},
                        "latex_ocr_status": {"type": "keyword"},
                        "latex_ocr_engine": {"type": "keyword"},
                        "latex_ocr_confidence": {"type": "float"},
                        "human_validation_status": {"type": "keyword"},
                        "use_for_codegen": {"type": "boolean"},
                        "human_validated": {"type": "boolean"},
                        "page": {"type": "integer"},
                        "bbox": {"type": "float"},
                        "source": {"type": "keyword"},
                        "pattern": {"type": "keyword"},
                        "confidence": {"type": "float"},
                    },
                },
                "citations": {
                    "type": "nested",
                    "properties": {
                        "apa": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 1024}}},
                        "doi": {"type": "keyword"},
                        "url": {"type": "keyword"},
                        "validated": {"type": "boolean"},
                    },
                },
                "validation_results": {
                    "type": "nested",
                    "properties": {
                        "check": {"type": "keyword"},
                        "status": {"type": "keyword"},
                        "command": {"type": "text"},
                        "passed": {"type": "boolean"},
                    },
                },
                "metadata": {"type": "object", "enabled": True},
            },
        },
    }


def mapping_contract_violations(mapping: dict[str, Any], vector_field: str = "embedding", dimension: int = 768) -> list[str]:
    """Return human-readable OpenSearch mapping contract violations."""

    violations: list[str] = []
    index_settings = mapping.get("settings", {}).get("index", {})
    props = mapping.get("mappings", {}).get("properties", {})
    if index_settings.get("knn") is not True:
        violations.append("settings.index.knn must be true")
    for field_name in ["doc_id", "paper_id", "chunk_id", "formula_id", "run_id", "source_type", "status", "sha256"]:
        if props.get(field_name, {}).get("type") != "keyword":
            violations.append(f"{field_name} must be keyword")
    for field_name in ["title", "abstract", "text", "chunk_text", "formula_description", "technical_summary"]:
        if props.get(field_name, {}).get("type") != "text":
            violations.append(f"{field_name} must be text")
    for field_name in [vector_field, "formula_embedding"]:
        cfg = props.get(field_name, {})
        if cfg.get("type") != "knn_vector":
            violations.append(f"{field_name} must be knn_vector")
        if int(cfg.get("dimension", -1)) != int(dimension):
            violations.append(f"{field_name} dimension must be {dimension}")
        method = cfg.get("method", {})
        if method.get("engine") != "faiss" or method.get("name") != "hnsw":
            violations.append(f"{field_name} must use FAISS/HNSW")
        if cfg.get("space_type") != "cosinesimil":
            violations.append(f"{field_name} must use cosinesimil")
    if props.get("formulas", {}).get("type") != "nested":
        violations.append("formulas must be nested")
    else:
        formula_props = props["formulas"].get("properties", {})
        if formula_props.get("latex", {}).get("type") != "text" or "raw" not in formula_props.get("latex", {}).get("fields", {}):
            violations.append("formulas.latex must be text with raw keyword subfield")
        for field_name in ["formula_id", "formula_image_path", "formula_image_status", "latex_ocr_status", "human_validation_status", "source", "pattern"]:
            if formula_props.get(field_name, {}).get("type") != "keyword":
                violations.append(f"formulas.{field_name} must be keyword")
    if props.get("citations", {}).get("type") != "nested":
        violations.append("citations must be nested")
    return violations


def assert_vector_dimension(mapping: dict[str, Any], vector_field: str, expected_dimension: int) -> None:
    props = mapping.get("mappings", {}).get("properties", {})
    actual = props.get(vector_field, {}).get("dimension")
    if int(actual) != int(expected_dimension):
        raise ValueError(f"vector field {vector_field} dimension mismatch: expected {expected_dimension}, got {actual}")


def build_alias_actions(
    existing_aliases: dict[str, Any],
    target_index: str,
    read_alias: str,
    write_alias: str,
    *,
    force_alias_update: bool = True,
) -> list[dict[str, Any]]:
    """Return OpenSearch alias actions matching the Rust migration contract."""

    actions: list[dict[str, Any]] = []
    if force_alias_update:
        for index_name, body in existing_aliases.items():
            aliases = body.get("aliases", {}) if isinstance(body, dict) else {}
            for alias in (read_alias, write_alias):
                if alias in aliases:
                    actions.append({"remove": {"index": index_name, "alias": alias}})
    actions.append({"add": {"index": target_index, "alias": read_alias}})
    actions.append({"add": {"index": target_index, "alias": write_alias, "is_write_index": True}})
    return actions


@dataclass
class MockOpenSearchTransport:
    existing_indices: set[str] = field(default_factory=set)
    aliases: dict[str, Any] = field(default_factory=dict)
    calls: list[dict[str, Any]] = field(default_factory=list)

    def migrate(
        self,
        *,
        target_index: str,
        mapping: dict[str, Any],
        read_alias: str,
        write_alias: str,
        force_alias_update: bool = True,
        dry_run: bool = False,
    ) -> dict[str, Any]:
        plan = {
            "target_index": target_index,
            "read_alias": read_alias,
            "write_alias": write_alias,
            "mapping_meta": mapping.get("mappings", {}).get("_meta", {}),
        }
        if dry_run:
            return {"ok": True, "dry_run": True, "plan": plan, "calls": list(self.calls)}
        if target_index not in self.existing_indices:
            self.calls.append({"method": "PUT", "path": f"/{target_index}", "body": mapping})
            self.existing_indices.add(target_index)
        else:
            self.calls.append({"method": "HEAD", "path": f"/{target_index}", "status": "already_exists"})
        actions = build_alias_actions(self.aliases, target_index, read_alias, write_alias, force_alias_update=force_alias_update)
        self.calls.append({"method": "POST", "path": "/_aliases", "body": {"actions": actions}})
        for action in actions:
            if "remove" in action:
                idx = action["remove"]["index"]
                alias = action["remove"]["alias"]
                self.aliases.setdefault(idx, {"aliases": {}}).setdefault("aliases", {}).pop(alias, None)
            elif "add" in action:
                idx = action["add"]["index"]
                alias = action["add"]["alias"]
                value = {k: v for k, v in action["add"].items() if k not in ("index", "alias")}
                self.aliases.setdefault(idx, {"aliases": {}}).setdefault("aliases", {})[alias] = value
        return {"ok": True, "dry_run": False, "plan": plan, "actions": actions, "calls": list(self.calls), "aliases": self.aliases}


def retrieval_fallback(search_responses: dict[str, dict[str, Any]], targets: Iterable[str]) -> dict[str, Any]:
    """Simulate OpenSearch target fallback for read/write/base/versioned targets."""

    attempts: list[dict[str, Any]] = []
    for target in targets:
        response = search_responses.get(target, {"ok": False, "error": "missing_mock_response"})
        if response.get("ok"):
            return {"ok": True, "target": target, "result": response.get("result", {}), "attempts": attempts}
        attempts.append({"target": target, "error": response.get("error", "failed")})
    return {"ok": False, "error": "all_targets_failed", "attempts": attempts}


def named_graph_uris(
    *,
    ontology_graph: str = "urn:veritas:graph:ontology",
    document_base: str = "urn:veritas:graph:document",
    run_base: str = "urn:veritas:graph:run",
    validation_base: str = "urn:veritas:graph:validation",
    document_hash: str = "sha256_fixture",
    run_id: str = "run-fixture",
) -> dict[str, str]:
    def safe(value: str) -> str:
        return "".join(ch if ch.isalnum() or ch in "_-" else "_" for ch in value).strip("_") or "item"

    return {
        "ontology": ontology_graph,
        "document": f"{document_base}:{safe(document_hash)}",
        "run": f"{run_base}:{safe(run_id)}",
        "validation": f"{validation_base}:{safe(run_id)}",
    }


def graph_store_request(graph_uri: str, turtle: str, *, replace: bool = False) -> dict[str, Any]:
    if not graph_uri.startswith("urn:veritas:graph:"):
        raise ValueError("graph_uri must use Veritas named graph policy")
    if not turtle.strip():
        raise ValueError("turtle must be non-empty")
    if contains_pdf_binary(turtle):
        raise ValueError("Fuseki graph upload must not contain PDF binary payloads")
    return {
        "method": "PUT" if replace else "POST",
        "params": {"graph": graph_uri},
        "headers": {"Content-Type": "text/turtle"},
        "body": turtle,
    }


def contains_pdf_binary(payload: str | bytes) -> bool:
    if isinstance(payload, bytes):
        head = payload[:16]
        return head.startswith(b"%PDF") or b"\x00" in payload[:1024]
    snippet = payload[:2048]
    return snippet.startswith("%PDF") or "JVBERi0" in snippet or "\u0000" in snippet


def run_report_to_turtle(run_id: str, report: dict[str, Any]) -> str:
    """Produce RDF facts for a run report source/build/validation summary."""

    def safe(value: str) -> str:
        return "".join(ch if ch.isalnum() or ch in "_-" else "_" for ch in value).strip("_") or "item"

    def lit(value: Any) -> str:
        return str(value).replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ")

    run = safe(run_id)
    status = report.get("final_status", "unknown")
    lines = [
        "@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .",
        f'<urn:veritas:run:{run}> a veritas:PlannedEngineeringAct ; veritas:hasIdentifier "{lit(run_id)}" ; veritas:hasStatus "{lit(status)}" .',
    ]
    for idx, path in enumerate(report.get("files_changed", [])):
        lines.append(f'<urn:veritas:source:{run}:{idx}> a veritas:SourceCodeArtifact ; veritas:hasIdentifier "{lit(path)}" ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:validatedBy <urn:veritas:validation:{run}:0> ; veritas:testedBy <urn:veritas:test:{run}:{idx}> .')
        lines.append(f'<urn:veritas:test:{run}:{idx}> a veritas:TestSpecification .')
    for idx, validation in enumerate(report.get("validation_results", [])):
        passed = bool(validation.get("success") or validation.get("passed"))
        lines.append(f'<urn:veritas:validation:{run}:{idx}> a veritas:VerificationResult ; veritas:hasStatus "{lit("passed" if passed else "failed")}" ; veritas:derivedFrom <urn:veritas:run:{run}> .')
    if status == "production_candidate_validated":
        lines.append(f'<urn:veritas:build:{run}> a veritas:BuildArtifact ; veritas:hasStatus "production_candidate_validated" ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:validatedBy <urn:veritas:validation:{run}:0> .')
    return "\n".join(lines) + "\n"


def flatten_sparql_binding(binding: dict[str, Any]) -> dict[str, Any]:
    flat: dict[str, Any] = {}
    for key, value in binding.items():
        if isinstance(value, dict) and "value" in value:
            flat[key] = value["value"]
        else:
            flat[key] = value
    return flat


def summarize_sparql_results(results_by_query: dict[str, dict[str, Any]], query_names: Iterable[str] = DEFAULT_QUERY_PACK_NAMES, sample_size: int = 5) -> dict[str, Any]:
    summaries: dict[str, Any] = {}
    warnings: list[dict[str, Any]] = []
    for name in query_names:
        result = results_by_query.get(name)
        if not result or not result.get("ok", True):
            warnings.append({"query": name, "code": "sparql.mock_missing_or_failed"})
            summaries[name] = {"ok": False, "count": 0, "samples": []}
            continue
        bindings = result.get("results", {}).get("bindings", [])
        samples = [flatten_sparql_binding(row) for row in bindings[:sample_size]]
        summaries[name] = {"ok": True, "count": len(bindings), "samples": samples}
    return {"queries": summaries, "warnings": warnings}


def fixture_sparql_results() -> dict[str, dict[str, Any]]:
    """Return deterministic SPARQL JSON fixtures for every planner query-pack item."""

    out: dict[str, dict[str, Any]] = {}
    for name in DEFAULT_QUERY_PACK_NAMES:
        out[name] = {
            "ok": True,
            "head": {"vars": ["item", "status"]},
            "results": {
                "bindings": [
                    {
                        "item": {"type": "uri", "value": f"urn:veritas:{name}:fixture"},
                        "status": {"type": "literal", "value": "requires_attention" if "without" in name else "available"},
                    }
                ]
            },
        }
    return out


def source_mocked_phase4_summary() -> dict[str, Any]:
    """Run the complete source/mocked retrieval + ontology hardening proof."""

    checks: list[dict[str, Any]] = []

    def record(name: str, ok: bool, details: Any | None = None) -> None:
        checks.append({"name": name, "ok": bool(ok), "details": details or {}})

    mapping = build_opensearch_mapping("embedding", 768, "v1")
    violations = mapping_contract_violations(mapping, "embedding", 768)
    record("opensearch_mapping_contract", not violations, {"violations": violations})
    try:
        assert_vector_dimension(mapping, "embedding", 1024)
        record("vector_dimension_mismatch_rejected", False, {"reason": "mismatch was not rejected"})
    except ValueError as exc:
        record("vector_dimension_mismatch_rejected", True, {"error": str(exc)})

    mock_os = MockOpenSearchTransport(aliases={"veritas-evidence-old": {"aliases": {"veritas-evidence-read": {}, "veritas-evidence-write": {"is_write_index": True}}}})
    migration = mock_os.migrate(target_index="veritas-evidence-v1", mapping=mapping, read_alias="veritas-evidence-read", write_alias="veritas-evidence-write")
    second = mock_os.migrate(target_index="veritas-evidence-v1", mapping=mapping, read_alias="veritas-evidence-read", write_alias="veritas-evidence-write")
    record("opensearch_migration_alias_update", any("remove" in action for action in migration["actions"]) and any("add" in action for action in migration["actions"]), {"actions": migration["actions"]})
    record("opensearch_migration_idempotent_second_run", any(call.get("status") == "already_exists" for call in second["calls"]), {"calls": second["calls"][-3:]})

    fallback = retrieval_fallback(
        {
            "veritas-evidence-read": {"ok": False, "error": "alias missing"},
            "veritas-evidence-write": {"ok": True, "result": {"hits": {"hits": [{"_id": "chunk-1"}]}}},
        },
        ["veritas-evidence-read", "veritas-evidence-write", "veritas-evidence", "veritas-evidence-v1"],
    )
    record("opensearch_retrieval_fallback_read_to_write", fallback.get("ok") and fallback.get("target") == "veritas-evidence-write", fallback)

    graphs = named_graph_uris(document_hash="sha256:abc123", run_id="run-001")
    record("fuseki_named_graphs_distinct", len(set(graphs.values())) == 4 and all(value.startswith("urn:veritas:graph:") for value in graphs.values()), graphs)

    turtle = "@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n<urn:veritas:doc:1> a veritas:SourceDocument ; veritas:hasSourceUrl \"data/papers/paper.pdf\" .\n"
    request = graph_store_request(graphs["document"], turtle, replace=False)
    record("fuseki_graph_store_upload_contract", request["method"] == "POST" and request["params"]["graph"] == graphs["document"], request)
    try:
        graph_store_request(graphs["document"], "%PDF-1.7 binary payload", replace=False)
        record("fuseki_rejects_pdf_binary_payload", False, {"reason": "PDF payload accepted"})
    except ValueError as exc:
        record("fuseki_rejects_pdf_binary_payload", True, {"error": str(exc)})

    run_ttl = run_report_to_turtle("run-001", {"final_status": "production_candidate_validated", "files_changed": ["src/lib.rs"], "validation_results": [{"success": True}]})
    record("run_report_rdf_contains_source_build_validation", all(token in run_ttl for token in ["SourceCodeArtifact", "BuildArtifact", "VerificationResult"]), {"ttl": run_ttl})

    summary = summarize_sparql_results(fixture_sparql_results())
    record("planner_sparql_fact_summary_all_queries", set(DEFAULT_QUERY_PACK_NAMES) <= set(summary["queries"].keys()) and all(value["count"] >= 1 for value in summary["queries"].values()), summary)

    return {"ok": all(item["ok"] for item in checks), "checks": checks, "summary": {"checks": len(checks), "query_pack_names": DEFAULT_QUERY_PACK_NAMES}}


if __name__ == "__main__":
    print(json.dumps(source_mocked_phase4_summary(), indent=2, sort_keys=True))
