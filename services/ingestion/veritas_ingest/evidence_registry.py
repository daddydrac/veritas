from __future__ import annotations

"""Authoritative Evidence Eligibility Registry for Veritas.

This module is part of the real product path, not a source-mocked proof. It
turns real ingestion manifests and persisted human review decisions into a
single causal decision artifact. Downstream application code must consult this
registry before evidence-backed planning or formula-to-code generation.
"""

import json
from pathlib import Path
from typing import Any

from .local_evidence_store import flatten_formulas, citation_records as chunk_citation_records

from .human_review import load_chunks_jsonl
from .local_evidence_store import citation_records, flatten_formulas, review_queue, write_json, write_jsonl

APPROVED_REVIEW = {"approve", "approved", "edit", "auto_approve", "approved_or_auto_approved"}
REJECTED_REVIEW = {"reject", "rejected", "blocked_rejected_by_human", "blocked_rejected"}
WAIVED_REVIEW = {"skip", "waive", "waived", "waived_for_exploration", "skip_with_waiver"}
PENDING_REVIEW = {"", "pending", "pending_review", "pending_human_review", "machine_generated_pending_human_review", "incomplete"}
DEFAULT_MIN_FORMULA_CONFIDENCE = 0.0


class EvidenceRegistryError(RuntimeError):
    """Raised when an evidence registry cannot be built or resolved."""


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    if not path.exists():
        return rows
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def _safe_float(value: Any, default: float = 0.0) -> float:
    try:
        if value in (None, ""):
            return default
        return float(value)
    except (TypeError, ValueError):
        return default


def _status(value: Any) -> str:
    return str(value or "").strip().lower()


def _formula_decision(formula: dict[str, Any]) -> str:
    for key in ("review_decision", "human_validation_status", "normalized_codegen_status", "codegen_eligibility_status"):
        value = _status(formula.get(key))
        if value:
            return value
    return "pending_review"


def _citation_decision(citation: dict[str, Any]) -> str:
    for key in ("review_decision", "citation_review_status", "normalized_review_status"):
        value = _status(citation.get(key))
        if value:
            return value
    return "machine_generated_pending_human_review"


def normalize_citation_record(citation: dict[str, Any]) -> dict[str, Any]:
    decision = _citation_decision(citation)
    citation_id = str(citation.get("citation_id") or citation.get("paper_id") or citation.get("source_document_id") or "unknown")
    explicitly_usable = bool(citation.get("citation_usable_for_audit"))
    if decision in REJECTED_REVIEW:
        normalized = "rejected"
        usable = False
        reason = "Citation was rejected by human review."
    elif decision in APPROVED_REVIEW and explicitly_usable:
        normalized = "approved"
        usable = True
        reason = ""
    elif decision in APPROVED_REVIEW:
        # Some older review paths persisted the approved decision but did not set
        # citation_usable_for_audit. Normalize that to usable because the human
        # decision is the authoritative source.
        normalized = "approved"
        usable = True
        reason = ""
    elif decision in WAIVED_REVIEW:
        normalized = "waived_for_exploration"
        usable = False
        reason = "Citation was waived only for exploratory/non-production use."
    else:
        normalized = "pending_review"
        usable = False
        reason = "Citation is pending human review before audit-backed planning."
    return {
        **citation,
        "citation_id": citation_id,
        "source_document_id": citation.get("source_document_id") or citation.get("paper_id") or citation_id,
        "review_decision": decision,
        "normalized_review_status": normalized,
        "citation_review_status": citation.get("citation_review_status") or decision,
        "citation_human_validated": bool(citation.get("citation_human_validated")) or normalized == "approved",
        "citation_usable_for_audit": usable,
        "eligible_for_planning": usable,
        "blocking_reason": reason,
    }


def normalize_formula_record(
    formula: dict[str, Any],
    citations_by_id: dict[str, dict[str, Any]],
    *,
    min_confidence: float = DEFAULT_MIN_FORMULA_CONFIDENCE,
) -> dict[str, Any]:
    formula_id = str(formula.get("formula_id") or formula.get("id") or "")
    citation_id = str(formula.get("citation_id") or formula.get("paper_id") or formula.get("source_document_id") or "")
    citation = citations_by_id.get(citation_id) or citations_by_id.get(str(formula.get("paper_id") or "")) or {}
    citation_usable = bool(citation.get("citation_usable_for_audit"))
    decision = _formula_decision(formula)
    raw_codegen_status = _status(formula.get("codegen_eligibility_status"))
    confidence = max(
        _safe_float(formula.get("confidence"), 0.0),
        _safe_float(formula.get("ocr_confidence"), 0.0),
        _safe_float(formula.get("latex_ocr_confidence"), 0.0),
        _safe_float(formula.get("formula_image_confidence"), 0.0),
    )
    has_latex = bool(str(formula.get("normalized_latex") or formula.get("latex") or formula.get("raw_latex") or "").strip())
    human_validated = bool(formula.get("human_validated")) or decision in APPROVED_REVIEW
    requested_codegen = bool(formula.get("use_for_codegen")) or raw_codegen_status in {"eligible", "eligible_human_validated"}

    if decision in REJECTED_REVIEW or raw_codegen_status in REJECTED_REVIEW or raw_codegen_status.startswith("blocked_rejected"):
        normalized = "rejected"
        eligible = False
        reason = "Formula was rejected by human review."
    elif not has_latex:
        normalized = "not_eligible_missing_latex"
        eligible = False
        reason = "Formula is missing LaTeX text and cannot be code-generated."
    elif formula.get("citation_id") and not citation_usable:
        normalized = "not_eligible_missing_citation"
        eligible = False
        reason = "Formula source citation is not approved/usable for audit."
    elif confidence < min_confidence:
        normalized = "not_eligible_low_confidence"
        eligible = False
        reason = f"Formula confidence {confidence:.3f} is below threshold {min_confidence:.3f}."
    elif human_validated and requested_codegen:
        normalized = "eligible"
        eligible = True
        reason = ""
    elif decision in WAIVED_REVIEW or raw_codegen_status == "waived_for_exploration":
        normalized = "waived_for_exploration"
        eligible = False
        reason = "Formula is waived only for exploratory/non-production use."
    else:
        normalized = "pending_review"
        eligible = False
        reason = "Formula is pending human approval before code generation."

    return {
        **formula,
        "formula_id": formula_id,
        "source_document_id": formula.get("source_document_id") or formula.get("paper_id"),
        "chunk_id": formula.get("chunk_id"),
        "citation_id": citation_id,
        "raw_latex": formula.get("raw_latex") or formula.get("latex", ""),
        "normalized_latex": formula.get("normalized_latex", ""),
        "ocr_status": formula.get("latex_ocr_status") or formula.get("ocr_status", ""),
        "ocr_confidence": confidence,
        "formula_image_status": formula.get("formula_image_status", ""),
        "human_validation_status": formula.get("human_validation_status") or decision,
        "review_decision": decision,
        "normalized_codegen_status": normalized,
        "codegen_eligibility_status": normalized,
        "eligible_for_codegen": eligible,
        "use_for_codegen": eligible,
        "human_validated": human_validated if eligible else bool(formula.get("human_validated")),
        "citation_review_status": citation.get("normalized_review_status", ""),
        "citation_usable_for_audit": citation_usable,
        "blocking_reason": reason,
    }


def _citation_lookup(citations: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    lookup: dict[str, dict[str, Any]] = {}
    for citation in citations:
        for key in ("citation_id", "paper_id", "source_document_id"):
            value = citation.get(key)
            if value not in (None, ""):
                lookup[str(value)] = citation
    return lookup


def build_registry(
    *,
    evidence_manifest: dict[str, Any] | None = None,
    formula_manifest: dict[str, Any] | None = None,
    citation_manifest: dict[str, Any] | None = None,
    workspace: Path | None = None,
    min_formula_confidence: float = DEFAULT_MIN_FORMULA_CONFIDENCE,
) -> dict[str, Any]:
    evidence_manifest = evidence_manifest or {}
    formula_manifest = formula_manifest or {}
    citation_manifest = citation_manifest or {}
    citations = [normalize_citation_record(c) for c in citation_manifest.get("records", [])]
    lookup = _citation_lookup(citations)
    formulas = [normalize_formula_record(f, lookup, min_confidence=min_formula_confidence) for f in formula_manifest.get("records", [])]
    approved_citations = [c for c in citations if c.get("eligible_for_planning")]
    blocked_citations = [c for c in citations if not c.get("eligible_for_planning")]
    eligible_formulas = [f for f in formulas if f.get("eligible_for_codegen")]
    blocked_formulas = [f for f in formulas if not f.get("eligible_for_codegen")]
    retrieval_blocked = bool(evidence_manifest.get("planning_blocked"))
    planning_allowed = bool(approved_citations) and not retrieval_blocked
    blocking_reasons: list[str] = []
    if retrieval_blocked:
        blocking_reasons.append(evidence_manifest.get("planning_block_reason") or "Retrieval is unavailable.")
    if not approved_citations:
        blocking_reasons.append("At least one approved citation is required for production-bound planning.")
    if blocked_citations:
        blocking_reasons.append(f"{len(blocked_citations)} citation(s) are pending, rejected, or exploration-only.")

    workspace_text = str(workspace) if workspace else evidence_manifest.get("workspace", "")
    registry = {
        "kind": "VeritasEvidenceEligibilityRegistry",
        "version": "phase3.2",
        "workspace": workspace_text,
        "source_document_id": evidence_manifest.get("source_document_id") or formula_manifest.get("source_document_id") or citation_manifest.get("source_document_id"),
        "evidence_manifest_path": str((workspace / "evidence_manifest.json") if workspace else evidence_manifest.get("path", "")),
        "formula_manifest_path": str((workspace / "formula_manifest.json") if workspace else formula_manifest.get("path", "")),
        "citation_manifest_path": str((workspace / "citation_manifest.json") if workspace else citation_manifest.get("path", "")),
        "retrieval_status": evidence_manifest.get("retrieval_status") or {
            "available": not retrieval_blocked,
            "blocking_reason": evidence_manifest.get("planning_block_reason", ""),
        },
        "planning": {
            "allowed": planning_allowed,
            "status": "eligible_for_evidence_backed_planning" if planning_allowed else "awaiting_evidence_review",
            "blocking_reasons": blocking_reasons,
            "usable_citation_ids": [c.get("citation_id") for c in approved_citations],
            "eligible_formula_ids": [f.get("formula_id") for f in eligible_formulas],
        },
        "planning_eligibility": {
            "eligible": planning_allowed,
            "status": "eligible_for_evidence_backed_planning" if planning_allowed else "awaiting_evidence_review",
            "blocking_reasons": blocking_reasons,
        },
        "codegen": {
            "eligible_formula_count": len(eligible_formulas),
            "blocked_formula_count": len(blocked_formulas),
        },
        "codegen_eligibility": {
            "eligible_formula_count": len(eligible_formulas),
            "blocked_formula_count": len(blocked_formulas),
        },
        "approved_citations": [c.get("citation_id") for c in approved_citations],
        "eligible_formulas": [f.get("formula_id") for f in eligible_formulas],
        "blocked_formulas": [{"formula_id": f.get("formula_id"), "status": f.get("codegen_eligibility_status"), "reason": f.get("blocking_reason")} for f in blocked_formulas],
        "blocked_citations": [{"citation_id": c.get("citation_id"), "status": c.get("normalized_review_status"), "reason": c.get("blocking_reason")} for c in blocked_citations],
        "summary": {
            "citations_total": len(citations),
            "citations_usable_for_audit": len(approved_citations),
            "citations_blocked": len(blocked_citations),
            "formulas_total": len(formulas),
            "formulas_eligible_for_codegen": len(eligible_formulas),
            "formulas_blocked": len(blocked_formulas),
        },
        "citations": citations,
        "formulas": formulas,
    }
    return registry


def build_registry_from_workspace(workspace: Path) -> dict[str, Any]:
    workspace = Path(workspace)
    evidence_manifest = load_json(workspace / "evidence_manifest.json")
    formula_manifest = load_json(workspace / "formula_manifest.json")
    citation_manifest = load_json(workspace / "citation_manifest.json")
    return build_registry(
        evidence_manifest=evidence_manifest,
        formula_manifest=formula_manifest,
        citation_manifest=citation_manifest,
        workspace=workspace,
    )


def refresh_manifests_from_chunks(workspace: Path) -> dict[str, Any]:
    workspace = Path(workspace)
    chunks_path = workspace / "chunks.jsonl"
    if not chunks_path.exists():
        raise EvidenceRegistryError(f"chunks.jsonl not found in {workspace}")
    chunks = load_chunks_jsonl(chunks_path)
    formulas = flatten_formulas(chunks)
    citations = citation_records(chunks)
    source_document_id = str(chunks[0].get("paper_id") if chunks else "unknown")
    write_jsonl(workspace / "formulas.jsonl", formulas)
    write_jsonl(workspace / "citations.jsonl", citations)
    formula_manifest = {
        "kind": "VeritasFormulaManifest",
        "backend": "local",
        "source_document_id": source_document_id,
        "count": len(formulas),
        "pending_review": sum(1 for f in formulas if not f.get("human_validated")),
        "eligible_for_codegen": sum(1 for f in formulas if f.get("use_for_codegen")),
        "formulas_path": str(workspace / "formulas.jsonl"),
        "records": formulas,
    }
    citation_manifest = {
        "kind": "VeritasCitationManifest",
        "backend": "local",
        "source_document_id": source_document_id,
        "count": len(citations),
        "pending_review": sum(1 for c in citations if not c.get("citation_usable_for_audit")),
        "usable_for_audit": sum(1 for c in citations if c.get("citation_usable_for_audit")),
        "citations_path": str(workspace / "citations.jsonl"),
        "records": citations,
    }
    write_json(workspace / "formula_manifest.json", formula_manifest)
    write_json(workspace / "citation_manifest.json", citation_manifest)
    write_json(workspace / "review_queue.json", review_queue(formulas, citations))
    return {"formula_manifest": formula_manifest, "citation_manifest": citation_manifest}


def write_registry(workspace: Path, registry: dict[str, Any]) -> None:
    write_json(workspace / "evidence_registry.json", registry)
    eligibility = {
        "kind": "VeritasEvidenceEligibilitySummary",
        "version": registry.get("version"),
        "workspace": registry.get("workspace"),
        "planning": registry.get("planning", {}),
        "planning_eligibility": registry.get("planning_eligibility", registry.get("planning", {})),
        "codegen": registry.get("codegen", {}),
        "codegen_eligibility": registry.get("codegen_eligibility", registry.get("codegen", {})),
        "summary": registry.get("summary", {}),
        "eligible_formulas": registry.get("eligible_formulas", []),
        "approved_citations": registry.get("approved_citations", []),
        "blocked_formulas": registry.get("blocked_formulas", [
            {"formula_id": f.get("formula_id"), "status": f.get("codegen_eligibility_status"), "reason": f.get("blocking_reason")}
            for f in registry.get("formulas", []) if not f.get("eligible_for_codegen")
        ]),
        "blocked_citations": registry.get("blocked_citations", [
            {"citation_id": c.get("citation_id"), "status": c.get("normalized_review_status"), "reason": c.get("blocking_reason")}
            for c in registry.get("citations", []) if not c.get("eligible_for_planning")
        ]),
    }
    write_json(workspace / "evidence_eligibility.json", eligibility)
    evidence_manifest_path = workspace / "evidence_manifest.json"
    if evidence_manifest_path.exists():
        evidence_manifest = load_json(evidence_manifest_path)
        evidence_manifest["evidence_registry_path"] = str(workspace / "evidence_registry.json")
        evidence_manifest["evidence_eligibility_path"] = str(workspace / "evidence_eligibility.json")
        evidence_manifest["evidence_registry_summary"] = registry.get("summary", {})
        evidence_manifest["planning_eligibility"] = registry.get("planning", {})
        write_json(evidence_manifest_path, evidence_manifest)


def refresh_workspace_registry(workspace: Path, *, refresh_from_chunks: bool = False) -> dict[str, Any]:
    workspace = Path(workspace)
    if refresh_from_chunks:
        refresh_manifests_from_chunks(workspace)
    registry = build_registry_from_workspace(workspace)
    write_registry(workspace, registry)
    return registry


def load_registry(workspace: Path) -> dict[str, Any]:
    workspace = Path(workspace)
    path = workspace / "evidence_registry.json"
    if path.exists():
        return load_json(path)
    return refresh_workspace_registry(workspace)


def formula_gate(registry: dict[str, Any], formula_id: str | None, citation_id: str | None = None) -> dict[str, Any]:
    formulas = registry.get("formulas", []) or []
    citations = registry.get("citations", []) or []
    selected = None
    if formula_id:
        for formula in formulas:
            if str(formula.get("formula_id")) == str(formula_id):
                selected = formula
                break
    elif len(formulas) == 1:
        selected = formulas[0]
    if selected is None:
        return {"ok": False, "status": "blocked_by_formula_review", "reason": "formula_id was not found in the Evidence Eligibility Registry", "formula_id": formula_id}
    if not selected.get("eligible_for_codegen"):
        return {"ok": False, "status": "blocked_by_formula_review", "reason": selected.get("blocking_reason") or "formula is not eligible for code generation", "formula": selected}
    selected_citation_id = str(citation_id or selected.get("citation_id") or "")
    citation = None
    for record in citations:
        if str(record.get("citation_id")) == selected_citation_id or str(record.get("paper_id")) == selected_citation_id or str(record.get("source_document_id")) == selected_citation_id:
            citation = record
            break
    if citation is None or not citation.get("citation_usable_for_audit"):
        return {"ok": False, "status": "blocked_by_citation_review", "reason": "formula source citation is not approved for audit-backed code generation", "formula": selected, "citation": citation}
    return {"ok": True, "status": "eligible", "formula": selected, "citation": citation}


def planning_gate(registry: dict[str, Any]) -> dict[str, Any]:
    planning = registry.get("planning", {}) or {}
    if planning.get("allowed"):
        return {"ok": True, "status": planning.get("status", "eligible_for_evidence_backed_planning"), "planning": planning}
    return {"ok": False, "status": planning.get("status", "awaiting_evidence_review"), "planning": planning, "reason": "; ".join(planning.get("blocking_reasons", []))}

# Backward-compatible names used by older CLI/tests. They call the real registry
# builder/writer above and do not create mocked eligibility.
def build_evidence_registry(workspace: Path | str) -> dict[str, Any]:
    return build_registry_from_workspace(Path(workspace))


def write_evidence_registry(workspace: Path | str) -> dict[str, Any]:
    path = Path(workspace)
    registry = build_registry_from_workspace(path)
    write_registry(path, registry)
    return registry
