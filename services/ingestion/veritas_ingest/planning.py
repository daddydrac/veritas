from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass, asdict
from typing import Any

import requests

from .embeddings import l2_norm, normalize_vector
from .errors import VeritasFailure


FORMULA_TRACE_QUERY = """
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
PREFIX dcterms: <http://purl.org/dc/terms/>
SELECT ?formula ?expr ?chunk ?paper ?title
WHERE {
  ?formula a veritas:SymbolicShadow ;
           veritas:hasExpressionText ?expr ;
           veritas:derivedFrom ?chunk .
  OPTIONAL { ?chunk veritas:derivedFrom ?paper . }
  OPTIONAL { ?paper dcterms:title ?title . }
}
LIMIT 25
""".strip()

EVIDENCE_QUERY = """
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
PREFIX dcterms: <http://purl.org/dc/terms/>
SELECT ?chunk ?paper ?title ?text
WHERE {
  ?chunk a veritas:RetrievalResult ;
         veritas:derivedFrom ?paper ;
         veritas:hasText ?text .
  OPTIONAL { ?paper dcterms:title ?title . }
}
LIMIT 25
""".strip()


@dataclass(frozen=True)
class PlanningEvidence:
    """Represent evidence used to ground a Veritas plan.

    Acceptance criteria:
        1. Preserve OpenSearch evidence hits.
        2. Preserve ontology/SPARQL formula evidence.
        3. Keep warning messages instead of hiding partial failures.
    """

    opensearch_hits: list[dict[str, Any]]
    formula_bindings: list[dict[str, Any]]
    graph_bindings: list[dict[str, Any]]
    warnings: list[dict[str, Any]]


def _api_url() -> str:
    return os.getenv("VERITAS_API_URL", "http://api:8080")


def _opensearch_url() -> str:
    return os.getenv("VERITAS_OPENSEARCH_URL", "http://opensearch:9200")


def _opensearch_index() -> str:
    return os.getenv("VERITAS_OPENSEARCH_INDEX", "veritas-papers")


def _embedding_url() -> str:
    return os.getenv("VERITAS_EMBEDDING_URL", "http://embedding:8090")


def _fuseki_query_url() -> str:
    return os.getenv("VERITAS_FUSEKI_QUERY_URL", "http://fuseki:3030/veritas/sparql")


def _embedding_field(cfg: dict[str, Any]) -> str:
    return str(
        cfg.get("services", {})
        .get("opensearch", {})
        .get("vector", {})
        .get("field", "embedding")
    )


def _post_json(url: str, payload: dict[str, Any], *, timeout: int = 120) -> dict[str, Any]:
    try:
        response = requests.post(url, json=payload, timeout=timeout)
    except requests.RequestException as exc:
        raise VeritasFailure(
            stage="planning.http_request",
            message=f"Request failed before service response: {url}: {exc}",
            remediation="Run `veritas ready`, inspect Docker Compose service logs, and verify service URLs in .env/config.",
            details={"url": url},
        ) from exc
    if response.status_code >= 400:
        raise VeritasFailure(
            stage="planning.http_response",
            message=f"Service returned HTTP {response.status_code}: {response.text[:1000]}",
            remediation="Inspect the upstream service logs and confirm data has been ingested before planning.",
            details={"url": url, "status": response.status_code},
        )
    return response.json()


def _embed_query(query: str, cfg: dict[str, Any]) -> list[float]:
    response = _post_json(
        _embedding_url().rstrip("/") + "/embed",
        {"texts": [query], "normalize": True, "batch_size": 1},
        timeout=600,
    )
    vectors = response.get("vectors", [])
    if not vectors:
        raise VeritasFailure(
            stage="planning.embed_query",
            message="Embedding service returned no vector for the planning query.",
            remediation="Check embedding service health and retry with a non-empty prompt.",
        )
    vector = [float(value) for value in vectors[0]]
    norm = l2_norm(vector)
    if abs(norm - 1.0) > 0.001:
        vector = normalize_vector(vector)
        norm = l2_norm(vector)
    if abs(norm - 1.0) > 0.001:
        raise VeritasFailure(
            stage="planning.embed_query_norm",
            message=f"Planning query vector is not normalized for cosine search: norm={norm:.6f}.",
            remediation="Ensure the embedding service uses normalize_embeddings=True.",
        )
    return vector


def _opensearch_hybrid_body(query: str, vector: list[float], field: str, size: int) -> dict[str, Any]:
    return {
        "size": size,
        "query": {
            "bool": {
                "should": [
                    {"knn": {field: {"vector": vector, "k": size}}},
                    {
                        "multi_match": {
                            "query": query,
                            "fields": ["text^3", "title^4", "metadata.summary^2"],
                        }
                    },
                    {
                        "nested": {
                            "path": "formulas",
                            "query": {"match": {"formulas.latex": query}},
                            "score_mode": "max",
                        }
                    },
                ],
                "minimum_should_match": 1,
            }
        },
    }


def retrieve_opensearch_evidence(query: str, cfg: dict[str, Any], *, size: int = 8) -> list[dict[str, Any]]:
    """Return evidence hits from OpenSearch FAISS/HNSW + lexical/formula search.

    Acceptance criteria:
        1. Embed the query with normalized SBERT vectors.
        2. Search the configured FAISS/HNSW OpenSearch index.
        3. Include lexical and nested formula search clauses.
        4. Return concise source summaries for planner prompts.
    """

    vector = _embed_query(query, cfg)
    body = _opensearch_hybrid_body(query, vector, _embedding_field(cfg), size)
    index = _opensearch_index()
    url = f"{_opensearch_url().rstrip('/')}/{index}/_search"
    try:
        response = requests.post(url, json=body, timeout=120)
    except requests.RequestException as exc:
        raise VeritasFailure(
            stage="planning.retrieve_opensearch",
            message=f"OpenSearch evidence retrieval failed: {exc}",
            remediation="Run `veritas ready`; verify OpenSearch is healthy and the embedding service is available.",
        ) from exc
    if response.status_code >= 400:
        raise VeritasFailure(
            stage="planning.retrieve_opensearch",
            message=f"OpenSearch evidence retrieval returned HTTP {response.status_code}: {response.text[:1000]}",
            remediation="Ingest papers first, verify FAISS/HNSW mapping, and inspect `docker compose logs opensearch`.",
        )
    hits = response.json().get("hits", {}).get("hits", [])
    evidence: list[dict[str, Any]] = []
    for hit in hits:
        source = hit.get("_source", {})
        evidence.append(
            {
                "score": hit.get("_score"),
                "chunk_id": source.get("chunk_id"),
                "paper_id": source.get("paper_id"),
                "title": source.get("title") or source.get("metadata", {}).get("title"),
                "text_preview": str(source.get("text", ""))[:1200],
                "formula_count": len(source.get("formulas", []) or []),
                "formulas": [
                    formula.get("latex")
                    for formula in (source.get("formulas", []) or [])[:5]
                    if formula.get("latex")
                ],
                "embedding_norm": source.get("embedding_norm"),
                "embedding_model": source.get("embedding_model"),
            }
        )
    return evidence


def _sparql(query: str) -> list[dict[str, Any]]:
    try:
        response = requests.post(
            _fuseki_query_url(),
            data={"query": query},
            headers={"accept": "application/sparql-results+json"},
            timeout=120,
        )
    except requests.RequestException as exc:
        raise VeritasFailure(
            stage="planning.sparql_transport",
            message=f"Fuseki SPARQL request failed: {exc}",
            remediation="Run `veritas ready`; inspect `docker compose logs fuseki`; verify ontology/data graphs are loaded.",
        ) from exc
    if response.status_code >= 400:
        raise VeritasFailure(
            stage="planning.sparql_response",
            message=f"Fuseki returned HTTP {response.status_code}: {response.text[:1000]}",
            remediation="Verify SPARQL prefixes use https://github.com/daddydrac/veritas/ontology# and data has been ingested.",
        )
    payload = response.json()
    return payload.get("results", {}).get("bindings", [])


def _binding_value(binding: dict[str, Any], key: str) -> str | None:
    value = binding.get(key, {})
    if isinstance(value, dict):
        return value.get("value")
    return None


def retrieve_graph_evidence() -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Return formula and chunk evidence from Fuseki.

    Acceptance criteria:
        1. Use the Veritas ontology namespace consistently.
        2. Return formula-level SymbolicShadow evidence.
        3. Return chunk-level RetrievalResult evidence.
    """

    formula_rows = []
    for binding in _sparql(FORMULA_TRACE_QUERY):
        formula_rows.append(
            {
                "formula": _binding_value(binding, "formula"),
                "expr": _binding_value(binding, "expr"),
                "chunk": _binding_value(binding, "chunk"),
                "paper": _binding_value(binding, "paper"),
                "title": _binding_value(binding, "title"),
            }
        )
    chunk_rows = []
    for binding in _sparql(EVIDENCE_QUERY):
        chunk_rows.append(
            {
                "chunk": _binding_value(binding, "chunk"),
                "paper": _binding_value(binding, "paper"),
                "title": _binding_value(binding, "title"),
                "text_preview": (_binding_value(binding, "text") or "")[:900],
            }
        )
    return formula_rows, chunk_rows


def gather_planning_evidence(prompt: str, cfg: dict[str, Any], *, size: int = 8) -> PlanningEvidence:
    """Gather OpenSearch and SPARQL evidence for a prompt.

    Acceptance criteria:
        1. Try both vector/lexical search and ontology graph evidence.
        2. Do not hide partial failures; return warnings with stage and remediation.
        3. Require at least one evidence source for evidence-backed planning.
    """

    warnings: list[dict[str, Any]] = []
    hits: list[dict[str, Any]] = []
    formula_bindings: list[dict[str, Any]] = []
    graph_bindings: list[dict[str, Any]] = []
    try:
        hits = retrieve_opensearch_evidence(prompt, cfg, size=size)
    except VeritasFailure as exc:
        warnings.append({"source": "opensearch", **asdict(exc)})
    try:
        formula_bindings, graph_bindings = retrieve_graph_evidence()
    except VeritasFailure as exc:
        warnings.append({"source": "fuseki", **asdict(exc)})
    if not hits and not formula_bindings and not graph_bindings:
        raise VeritasFailure(
            stage="planning.no_evidence",
            message="Veritas could not gather evidence from OpenSearch or Fuseki.",
            remediation="Ingest arXiv papers or PDFs first, upload the ontology, then rerun the prompt. Use `veritas ready` and `veritas search` to diagnose retrieval.",
            details={"warnings": warnings},
        )
    return PlanningEvidence(hits, formula_bindings, graph_bindings, warnings)


def representation_first_analysis(prompt: str, evidence: PlanningEvidence) -> dict[str, Any]:
    """Return a representation-first analysis draft grounded in evidence.

    Acceptance criteria:
        1. Follow the MATH.md surface → representation → invariant workflow.
        2. Include evidence and uncertainty instead of pretending proof.
        3. Emit validation gates required before code generation is trusted.
    """

    formula_samples = [row.get("expr") for row in evidence.formula_bindings if row.get("expr")][:10]
    evidence_titles = sorted(
        {
            item.get("title")
            for item in evidence.opensearch_hits + evidence.graph_bindings
            if item.get("title")
        }
    )[:10]
    return {
        "status": "evidence_backed_planning_draft",
        "surface_phenomenon": {
            "prompt": prompt,
            "observed_sources": evidence_titles,
            "apparent_complexity": [
                "math-heavy PDF notation",
                "formula/prose alignment",
                "implementation ambiguity",
                "unknown assumptions and domains",
            ],
        },
        "representation_hypothesis": {
            "candidate_map": "PDF prose + LaTeX symbolic shadows -> typed research evidence graph -> implementation plan",
            "preserves": ["formula text", "chunk context", "paper provenance", "retrieval score", "ontology type"],
            "discards_or_defers": ["unverified proof status", "full formal semantics", "human review of novelty"],
        },
        "symbolic_shadows": formula_samples,
        "candidate_invariants": [
            "formula expressions must remain linked to source chunks",
            "generated code must cite evidence chunks",
            "generated tests must cover stated preconditions and postconditions",
        ],
        "compression_fidelity_gates": [
            "formula extraction confidence reviewed",
            "chunk contains complete formula boundaries",
            "embedding norm validated for cosine search",
            "SPARQL traceability query returns source formulas",
        ],
        "risk_register": [
            {
                "risk": "MathematicalRisk: formula semantics may be underspecified",
                "mitigation": "require symbolic-shadow review and explicit assumptions before codegen",
            },
            {
                "risk": "TechnicalRisk: generated code may not preserve numerical invariants",
                "mitigation": "generate property tests and tolerance checks from extracted constraints",
            },
            {
                "risk": "OperationalRisk: generated package may not match target CPU/GPU runtime",
                "mitigation": "generate runtime spec and validate package in container before release",
            },
        ],
        "validation_gates": [
            "evidence_required",
            "risk_register_required",
            "control_flow_backcheck_required",
            "tests_required",
            "package_validation_required",
        ],
    }


def build_evidence_backed_plan(prompt: str, cfg: dict[str, Any], *, size: int = 8) -> dict[str, Any]:
    evidence = gather_planning_evidence(prompt, cfg, size=size)
    analysis = representation_first_analysis(prompt, evidence)
    return {
        "ok": True,
        "kind": "VeritasEvidenceBackedPlan",
        "prompt": prompt,
        "evidence": asdict(evidence),
        "analysis": analysis,
        "execution_plan": [
            "select evidence chunks and formulas",
            "state assumptions, domains, and constraints",
            "define representation map and implementation target",
            "run autonomous package generation with tests and validation report",
            "run control-flow and risk checks",
            "emit final result or structured failure envelope",
        ],
        "next_actions": [
            "veritas generate-code --language rust --prompt '<your prompt>'",
            "veritas sparql '<query>'",
            "veritas search '<evidence query>'",
        ],
    }
