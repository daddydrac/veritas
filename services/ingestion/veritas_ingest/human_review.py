from __future__ import annotations

"""Human-in-the-loop review helpers for Veritas ingestion artifacts.

The review helpers are intentionally pure/data-oriented so they can be used by
interactive CLI flows, non-interactive CI, and downstream OpenSearch/Fuseki
persistence.  Review decisions are stored inside chunk metadata/formula metadata
rather than a side channel so ingestion can index and graph the same state.
"""

import json
from pathlib import Path
from typing import Any, Iterable

from .latex_ocr import normalize_latex

FORMULA_DECISIONS = {"approve", "edit", "reject", "skip", "auto_approve"}
CITATION_DECISIONS = {"approve", "edit", "reject", "skip", "incomplete", "auto_approve"}


def load_chunks_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def write_chunks_jsonl(path: Path, chunks: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(json.dumps(c, ensure_ascii=False) for c in chunks), encoding="utf-8")


def iter_formulas(chunks: list[dict[str, Any]]) -> Iterable[tuple[dict[str, Any], dict[str, Any]]]:
    for chunk in chunks:
        for formula in chunk.get("formulas", []) or []:
            yield chunk, formula


def apply_formula_decision(formula: dict[str, Any], decision: str, corrected_latex: str | None = None, reviewer: str = "human") -> dict[str, Any]:
    """Apply approve/edit/reject/skip decision to a formula object."""

    normalized = decision.strip().lower()
    if normalized not in FORMULA_DECISIONS:
        raise ValueError(f"Unsupported formula review decision: {decision}")
    formula["human_validation_status"] = normalized
    formula["human_validated"] = normalized in {"approve", "edit", "auto_approve"}
    formula["human_reviewer"] = reviewer
    formula["review_required"] = normalized not in {"approve", "edit", "auto_approve"}
    if normalized == "edit":
        if not corrected_latex:
            raise ValueError("Corrected LaTeX is required for edit decisions.")
        formula["original_latex"] = formula.get("latex", "")
        formula["latex"] = corrected_latex.strip()
        formula["normalized_latex"] = normalize_latex(corrected_latex)
    elif formula.get("latex"):
        formula.setdefault("normalized_latex", normalize_latex(str(formula.get("latex", ""))))
    if normalized == "reject":
        formula["use_for_codegen"] = False
        formula["codegen_eligibility_status"] = "blocked_rejected_by_human"
    elif normalized in {"approve", "edit", "auto_approve"}:
        formula["use_for_codegen"] = True
        formula["codegen_eligibility_status"] = "eligible_human_validated"
    else:
        formula["use_for_codegen"] = False
        formula["codegen_eligibility_status"] = "blocked_pending_human_review"
    return formula


def review_formulas_noninteractive(path: Path, decision: str, reviewer: str = "human", output: Path | None = None, corrected_latex: str | None = None) -> dict[str, Any]:
    chunks = load_chunks_jsonl(path)
    count = 0
    eligible = 0
    for _chunk, formula in iter_formulas(chunks):
        apply_formula_decision(formula, decision, corrected_latex=corrected_latex, reviewer=reviewer)
        count += 1
        if formula.get("use_for_codegen"):
            eligible += 1
    target = output or path
    write_chunks_jsonl(target, chunks)
    return {"ok": True, "formulas_reviewed": count, "codegen_eligible": eligible, "decision": decision, "path": str(target)}


def apply_citation_decision(metadata: dict[str, Any], decision: str, corrected_citation: str | None = None, reviewer: str = "human") -> dict[str, Any]:
    """Apply approve/edit/reject/incomplete decision to citation metadata."""

    normalized = decision.strip().lower()
    if normalized not in CITATION_DECISIONS:
        raise ValueError(f"Unsupported citation review decision: {decision}")
    if normalized == "edit":
        if not corrected_citation:
            raise ValueError("Corrected citation is required for edit decisions.")
        metadata["original_apa_citation"] = metadata.get("apa_citation", "")
        metadata["apa_citation"] = corrected_citation.strip()
    metadata["citation_review_status"] = normalized
    metadata["citation_human_validated"] = normalized in {"approve", "edit", "auto_approve"}
    metadata["citation_reviewer"] = reviewer
    if normalized == "reject":
        metadata["citation_usable_for_audit"] = False
    elif normalized in {"approve", "edit", "auto_approve"}:
        metadata["citation_usable_for_audit"] = True
    else:
        metadata["citation_usable_for_audit"] = False
    return metadata


def review_citations_in_chunks(path: Path, decision: str, reviewer: str = "human", output: Path | None = None, corrected_citation: str | None = None) -> dict[str, Any]:
    """Apply a citation decision to chunk metadata records.

    Each chunk carries a copy of paper metadata so downstream OpenSearch/RDF
    writers can persist citation status without joining side-car files.
    """

    chunks = load_chunks_jsonl(path)
    paper_ids: set[str] = set()
    for chunk in chunks:
        metadata = chunk.setdefault("metadata", {})
        apply_citation_decision(metadata, decision, corrected_citation=corrected_citation, reviewer=reviewer)
        paper_ids.add(str(chunk.get("paper_id") or metadata.get("paper_id") or "unknown"))
    target = output or path
    write_chunks_jsonl(target, chunks)
    return {"ok": True, "papers_reviewed": len(paper_ids), "chunks_updated": len(chunks), "decision": decision, "path": str(target)}


def review_summary(chunks: list[dict[str, Any]]) -> dict[str, Any]:
    formulas = [formula for _chunk, formula in iter_formulas(chunks)]
    citations = {str(chunk.get("paper_id") or chunk.get("metadata", {}).get("paper_id") or "unknown"): chunk.get("metadata", {}) for chunk in chunks}
    return {
        "formula_count": len(formulas),
        "formula_human_validated": sum(1 for f in formulas if f.get("human_validated") is True),
        "formula_codegen_eligible": sum(1 for f in formulas if f.get("use_for_codegen") is True),
        "formula_rejected": sum(1 for f in formulas if f.get("human_validation_status") == "reject"),
        "citation_count": len(citations),
        "citation_human_validated": sum(1 for m in citations.values() if m.get("citation_human_validated") is True),
        "citation_rejected": sum(1 for m in citations.values() if m.get("citation_review_status") == "reject"),
    }
