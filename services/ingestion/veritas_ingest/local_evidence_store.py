from __future__ import annotations

"""Evidence manifest writer for the real local-ingestion backend."""

import json
from pathlib import Path
from typing import Any

from .human_review import review_summary


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding="utf-8")


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rows), encoding="utf-8")


def flatten_formulas(chunks: list[dict[str, Any]]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    seen: set[str] = set()
    for chunk in chunks:
        meta = chunk.get("metadata", {}) or {}
        for formula in chunk.get("formulas", []) or []:
            formula_id = str(formula.get("formula_id") or "")
            if formula_id and formula_id in seen:
                continue
            if formula_id:
                seen.add(formula_id)
            records.append({
                **formula,
                "paper_id": chunk.get("paper_id"),
                "chunk_id": formula.get("chunk_id") or chunk.get("chunk_id"),
                "source_document_id": chunk.get("paper_id"),
                "citation_id": meta.get("citation_id") or meta.get("paper_id") or chunk.get("paper_id"),
                "citation_review_status": meta.get("citation_review_status", "machine_generated_pending_human_review"),
                "citation_usable_for_audit": bool(meta.get("citation_usable_for_audit", False)),
            })
    return records


def citation_records(chunks: list[dict[str, Any]]) -> list[dict[str, Any]]:
    by_paper: dict[str, dict[str, Any]] = {}
    for chunk in chunks:
        meta = chunk.get("metadata", {}) or {}
        paper_id = str(chunk.get("paper_id") or meta.get("paper_id") or "unknown")
        if paper_id not in by_paper:
            by_paper[paper_id] = {
                "citation_id": meta.get("citation_id") or paper_id,
                "source_document_id": paper_id,
                "paper_id": paper_id,
                "title": meta.get("title", ""),
                "authors": meta.get("authors", []),
                "year": meta.get("year", ""),
                "apa_citation": meta.get("apa_citation", ""),
                "source_url": meta.get("source_url") or meta.get("entry_url") or meta.get("pdf_url", ""),
                "citation_review_status": meta.get("citation_review_status", "machine_generated_pending_human_review"),
                "citation_human_validated": bool(meta.get("citation_human_validated", False)),
                "citation_usable_for_audit": bool(meta.get("citation_usable_for_audit", False)),
                "citation_reviewer": meta.get("citation_reviewer", ""),
            }
    return list(by_paper.values())


def review_queue(formulas: list[dict[str, Any]], citations: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "kind": "VeritasReviewQueue",
        "formula_review": [
            {
                "formula_id": f.get("formula_id"),
                "chunk_id": f.get("chunk_id"),
                "latex": f.get("latex", ""),
                "normalized_latex": f.get("normalized_latex", ""),
                "description": f.get("description", ""),
                "source": f.get("source", ""),
                "confidence": f.get("confidence"),
                "formula_image_path": f.get("formula_image_path", ""),
                "formula_image_status": f.get("formula_image_status", ""),
                "latex_ocr_status": f.get("latex_ocr_status", ""),
                "human_validation_status": f.get("human_validation_status", "pending"),
                "codegen_eligibility_status": f.get("codegen_eligibility_status", "blocked_pending_human_review"),
                "recommended_action": "approve/edit/reject/skip before production-bound formula-to-code",
            }
            for f in formulas
        ],
        "citation_review": [
            {
                "citation_id": c.get("citation_id"),
                "paper_id": c.get("paper_id"),
                "apa_citation": c.get("apa_citation", ""),
                "citation_review_status": c.get("citation_review_status", "machine_generated_pending_human_review"),
                "citation_usable_for_audit": c.get("citation_usable_for_audit", False),
                "recommended_action": "approve/edit/reject/incomplete before production-bound planning",
            }
            for c in citations
        ],
    }


def build_evidence_manifest(
    *,
    chunks: list[dict[str, Any]],
    formulas: list[dict[str, Any]],
    citations: list[dict[str, Any]],
    source_pdf: Path,
    paper_id: str,
    embedding_status: dict[str, Any],
    vector_index: dict[str, Any],
    lexical_index: dict[str, Any],
    rdf: dict[str, Any],
    workspace: Path,
) -> dict[str, Any]:
    formula_pending = sum(1 for f in formulas if not f.get("human_validated"))
    citation_pending = sum(1 for c in citations if not c.get("citation_usable_for_audit"))
    planning_blocked = bool(embedding_status.get("planning_blocked"))
    return {
        "kind": "VeritasEvidenceManifest",
        "backend": "local",
        "source_document_id": paper_id,
        "paper_id": paper_id,
        "source_pdf": str(source_pdf),
        "workspace": str(workspace),
        "chunks_path": str(workspace / "chunks.jsonl"),
        "formulas_path": str(workspace / "formulas.jsonl"),
        "citations_path": str(workspace / "citations.jsonl"),
        "review_queue_path": str(workspace / "review_queue.json"),
        "evidence_registry_path": str(workspace / "evidence_registry.json"),
        "rdf_path": rdf.get("path"),
        "graph_uri": rdf.get("graph_uri"),
        "lexical_index": lexical_index,
        "vector_index": vector_index,
        "embedding": embedding_status,
        "chunk_count": len(chunks),
        "formula_count": len(formulas),
        "citation_count": len(citations),
        "pending_formula_review": formula_pending,
        "pending_citation_review": citation_pending,
        "planning_status": "blocked_retrieval_unavailable" if planning_blocked else "available_after_review",
        "planning_blocked": planning_blocked,
        "planning_block_reason": "Local embeddings unavailable; production-bound planning requires embeddings." if planning_blocked else "",
        "next_actions": [
            "Review citations and formulas before production-bound planning.",
            embedding_status.get("remediation", "") if planning_blocked else "Run Veritas planning after review gates pass.",
        ],
        "review_summary": review_summary(chunks),
    }


def write_ingestion_report(workspace: Path, manifest: dict[str, Any]) -> None:
    lines = [
        "# Veritas Local Ingestion Report",
        "",
        f"Source document: `{manifest.get('source_pdf')}`",
        f"Paper ID: `{manifest.get('paper_id')}`",
        f"Chunks: {manifest.get('chunk_count')}",
        f"Formulas: {manifest.get('formula_count')}",
        f"Citations: {manifest.get('citation_count')}",
        f"Embedding status: {manifest.get('embedding', {}).get('status')}",
        f"Planning status: {manifest.get('planning_status')}",
        "",
        "## Next actions",
    ]
    for action in manifest.get("next_actions", []):
        if action:
            lines.append(f"- {action}")
    lines.extend([
        "",
        "## Artifacts",
        f"- Evidence manifest: `{workspace / 'evidence_manifest.json'}`",
        f"- Formula manifest: `{workspace / 'formula_manifest.json'}`",
        f"- Citation manifest: `{workspace / 'citation_manifest.json'}`",
        f"- Review queue: `{workspace / 'review_queue.json'}`",
        f"- Evidence eligibility registry: `{workspace / 'evidence_registry.json'}`",
        f"- RDF evidence: `{workspace / 'evidence.ttl'}`",
    ])
    (workspace / "ingestion_report.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
