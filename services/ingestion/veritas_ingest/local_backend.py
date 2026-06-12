from __future__ import annotations

"""Real local ingestion backend for Veritas Journey mode.

The backend parses real PDF-derived chunks produced by the existing Docling-first
pipeline, writes local JSONL/RDF/search artifacts, and records whether planning
must block because a real embedding provider is unavailable. It does not create
mock evidence or fake vectors.
"""

import copy
import os
from pathlib import Path
from typing import Any

from .evidence_registry import refresh_workspace_registry
from .local_embedding_provider import attach_local_embeddings
from .local_evidence_store import (
    build_evidence_manifest,
    citation_records,
    flatten_formulas,
    review_queue,
    write_ingestion_report,
    write_json,
    write_jsonl,
)
from .local_rdf_store import write_local_rdf
from .local_vector_store import write_local_lexical_index, write_local_vector_index

_WORKSPACE_PREFIX = "/workspace/"


def repo_local_workspace() -> Path:
    return Path(os.getenv("VERITAS_LOCAL_WORKSPACE", Path.cwd())).resolve()


def resolve_local_output_path(path_value: str | Path, *, local_root: Path | None = None) -> Path:
    path = Path(path_value)
    if local_root is None:
        local_root = repo_local_workspace()
    text = str(path)
    if text.startswith(_WORKSPACE_PREFIX):
        return local_root / text[len(_WORKSPACE_PREFIX):]
    if path.is_absolute():
        return path
    return local_root / path


def prepare_local_config(cfg: dict[str, Any], *, output_dir: Path | None = None) -> dict[str, Any]:
    local_cfg = copy.deepcopy(cfg)
    outputs = local_cfg.setdefault("ingestion", {}).setdefault("outputs", {})
    root = output_dir.resolve() if output_dir else resolve_local_output_path(outputs.get("chunks_dir", "data/local-ingestion"))
    root.mkdir(parents=True, exist_ok=True)
    outputs["chunks_dir"] = str(root)
    outputs["raw_pdf_dir"] = str(root / "papers")
    outputs["docling_dir"] = str(root / "docling")
    outputs["formula_image_dir"] = str(root / "formulas")
    local_cfg.setdefault("runtime", {})["ingestion_backend"] = "local"
    local_cfg["runtime"]["local_output_dir"] = str(root)
    return local_cfg


def local_backend_enabled(cfg: dict[str, Any]) -> bool:
    runtime = cfg.get("runtime", {}) or cfg.get("_runtime", {}) or {}
    env = os.getenv("VERITAS_INGESTION_BACKEND", "").strip().lower()
    backend = str(runtime.get("ingestion_backend") or env or cfg.get("ingestion", {}).get("backend", "")).strip().lower()
    return backend in {"local", "local_files", "filesystem"}


def write_local_outputs(
    *,
    chunks: list[dict[str, Any]],
    cfg: dict[str, Any],
    workspace: Path,
    source_pdf: Path,
    paper_id: str,
) -> dict[str, Any]:
    """Persist real local evidence/RDF/vector artifacts.

    The function performs no OpenSearch/Fuseki writes and never fabricates
    vectors. If a real embedding provider is not available, local ingestion
    still succeeds for evidence review, but the evidence manifest blocks
    production-bound planning with planning_status=blocked_retrieval_unavailable.
    """

    local_cfg = prepare_local_config(cfg, output_dir=workspace)
    root = Path(local_cfg["ingestion"]["outputs"]["chunks_dir"])
    root.mkdir(parents=True, exist_ok=True)

    chunks_with_embeddings, embedding_status = attach_local_embeddings(chunks, local_cfg)
    formulas = flatten_formulas(chunks_with_embeddings)
    citations = citation_records(chunks_with_embeddings)

    write_jsonl(root / "chunks.jsonl", chunks_with_embeddings)
    write_jsonl(root / "formulas.jsonl", formulas)
    write_jsonl(root / "citations.jsonl", citations)

    lexical_status = write_local_lexical_index(chunks_with_embeddings, root / "local_lexical_index.jsonl")
    vector_status = write_local_vector_index(chunks_with_embeddings, root / "local_vector_index.jsonl")
    rdf_status = write_local_rdf(chunks_with_embeddings, local_cfg, root, paper_id=paper_id)
    queue = review_queue(formulas, citations)
    write_json(root / "review_queue.json", queue)

    evidence_manifest = build_evidence_manifest(
        chunks=chunks_with_embeddings,
        formulas=formulas,
        citations=citations,
        source_pdf=source_pdf,
        paper_id=paper_id,
        embedding_status=embedding_status,
        vector_index=vector_status,
        lexical_index=lexical_status,
        rdf=rdf_status,
        workspace=root,
    )
    formula_manifest = {
        "kind": "VeritasFormulaManifest",
        "backend": "local",
        "source_document_id": paper_id,
        "count": len(formulas),
        "pending_review": evidence_manifest["pending_formula_review"],
        "eligible_for_codegen": sum(1 for f in formulas if f.get("use_for_codegen")),
        "formulas_path": str(root / "formulas.jsonl"),
        "records": formulas,
    }
    citation_manifest = {
        "kind": "VeritasCitationManifest",
        "backend": "local",
        "source_document_id": paper_id,
        "count": len(citations),
        "pending_review": evidence_manifest["pending_citation_review"],
        "usable_for_audit": sum(1 for c in citations if c.get("citation_usable_for_audit")),
        "citations_path": str(root / "citations.jsonl"),
        "records": citations,
    }

    write_json(root / "evidence_manifest.json", evidence_manifest)
    write_json(root / "formula_manifest.json", formula_manifest)
    write_json(root / "citation_manifest.json", citation_manifest)

    registry = refresh_workspace_registry(root)
    evidence_manifest = (root / "evidence_manifest.json")
    current_evidence_manifest = json_load(evidence_manifest)

    write_json(root / "local_vector_index_manifest.json", vector_status)
    write_json(root / "local_lexical_index_manifest.json", lexical_status)
    write_json(root / "latest-local-ingest-manifest.json", current_evidence_manifest)
    write_ingestion_report(root, current_evidence_manifest)

    return {
        "ok": True,
        "backend": "local",
        "workspace": str(root),
        "output_dir": str(root),
        "paper_id": paper_id,
        "chunks": len(chunks_with_embeddings),
        "evidence_manifest_path": str(root / "evidence_manifest.json"),
        "formula_manifest_path": str(root / "formula_manifest.json"),
        "citation_manifest_path": str(root / "citation_manifest.json"),
        "review_queue_path": str(root / "review_queue.json"),
        "evidence_registry_path": str(root / "evidence_registry.json"),
        "evidence_eligibility_path": str(root / "evidence_eligibility.json"),
        "ingestion_report_path": str(root / "ingestion_report.md"),
        "rdf_path": str(root / "evidence.ttl"),
        "lexical_index_path": str(root / "local_lexical_index.jsonl"),
        "vector_index_path": str(root / "local_vector_index.jsonl"),
        "embedding_status": embedding_status,
        "retrieval_status": {
            "available": not bool(embedding_status.get("planning_blocked")),
            "mode": "local_vector" if not bool(embedding_status.get("planning_blocked")) else "local_lexical_only_embeddings_unavailable",
            "blocking_reason": "" if not bool(embedding_status.get("planning_blocked")) else embedding_status.get("remediation", "Local embeddings unavailable."),
        },
        "planning_status": current_evidence_manifest.get("planning_status"),
        "planning_blocked": current_evidence_manifest.get("planning_blocked", False),
        "evidence_manifest": current_evidence_manifest,
        "formula_manifest": formula_manifest,
        "citation_manifest": citation_manifest,
        "evidence_registry": registry,
        "review_queue": queue,
    }


def json_load(path: Path) -> dict[str, Any]:
    import json

    return json.loads(path.read_text(encoding="utf-8"))
