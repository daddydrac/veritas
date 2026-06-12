from __future__ import annotations

"""Human checkpoint workflow for Veritas.

This module is intentionally pure/data-oriented.  It is used by source/mocked
E2E tests, ingestion CLI flows, and API/graph persistence adapters to model
researcher + machine teaming without requiring live services.

The workflow covers the full Veritas review chain:

    citation -> formula -> representation -> plan -> code architecture -> validation

A checkpoint can be approved, edited, rejected, auto-approved, skipped with a
reason (explicit waiver), or left pending.  Policy gates decide whether a run can
continue and whether an artifact may be marked production-candidate.
"""

import hashlib
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from rdflib import RDF, Graph, Literal, Namespace

CHECKPOINT_PHASES: tuple[str, ...] = (
    "citation_review",
    "formula_review",
    "representation_review",
    "plan_review",
    "code_architecture_review",
    "validation_review",
)

CHECKPOINT_DECISIONS = {
    "pending",
    "approve",
    "edit",
    "reject",
    "skip",
    "auto_approve",
    "ask_for_explanation",
}

APPROVAL_DECISIONS = {"approve", "edit", "auto_approve"}
BLOCKING_DECISIONS = {"reject", "pending", "ask_for_explanation"}
POLICIES = {"auto_approve", "require_all", "require_high_risk_only"}

PHASE_LABELS = {
    "citation_review": "Citation Review",
    "formula_review": "Formula Review",
    "representation_review": "Representation Review",
    "plan_review": "Plan Review",
    "code_architecture_review": "Code Architecture Review",
    "validation_review": "Validation Review",
}

PHASE_QUESTIONS = {
    "citation_review": "Do you approve the APA citation and source metadata for auditability?",
    "formula_review": "Do you approve the formula/OCR result as a SymbolicShadow eligible for downstream reasoning?",
    "representation_review": "Do you approve the representation-first mathematical interpretation, including surface phenomenon, representation map, invariants, risks, and transfer/proof status?",
    "plan_review": "Do you approve the planner steps, risks, assumptions, tool calls, and validation gates?",
    "code_architecture_review": "Do you approve the proposed source-file plan, package structure, functional-composition constraints, commands, and test strategy?",
    "validation_review": "Do you approve the final validation evidence before production-candidate status is assigned?",
}

HIGH_RISK_PHASES = {"representation_review", "plan_review", "code_architecture_review", "validation_review"}


def now_ms() -> int:
    return int(time.time() * 1000)


def stable_digest(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, ensure_ascii=False, default=str)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def normalize_decision(decision: str | None) -> str:
    d = (decision or "pending").strip().lower()
    if d not in CHECKPOINT_DECISIONS:
        raise ValueError(f"Unsupported human checkpoint decision: {decision}")
    return d


def normalize_policy(policy: str | None) -> str:
    p = (policy or "require_high_risk_only").strip().lower()
    if p not in POLICIES:
        raise ValueError(f"Unsupported human checkpoint policy: {policy}")
    return p


def artifact_is_high_risk(phase: str, artifact: dict[str, Any]) -> bool:
    text = json.dumps(artifact, sort_keys=True, default=str).lower()
    if phase in HIGH_RISK_PHASES:
        return True
    if any(token in text for token in ("critical", "high", "unsafe", "unproved", "unverified", "failed", "missing", "blocked", "risk")):
        return True
    if phase == "formula_review" and artifact.get("use_for_codegen") is True and not artifact.get("human_validated"):
        return True
    if phase == "citation_review" and not artifact.get("citation_human_validated"):
        return bool(artifact.get("review_required", False))
    return False


def phase_required(policy: str, phase: str, artifact: dict[str, Any], explicit_required: bool | None = None) -> bool:
    if explicit_required is not None:
        return bool(explicit_required)
    p = normalize_policy(policy)
    if p == "auto_approve":
        return False
    if p == "require_all":
        return True
    return artifact_is_high_risk(phase, artifact)


def decision_approved(decision: str, notes: str = "") -> bool:
    d = normalize_decision(decision)
    if d in APPROVAL_DECISIONS:
        return True
    if d == "skip" and notes.strip():
        return True
    return False


def decision_status(decision: str, required: bool, notes: str = "") -> str:
    d = normalize_decision(decision)
    if d in APPROVAL_DECISIONS:
        return "approved"
    if d == "skip" and notes.strip():
        return "waived_with_reason"
    if d == "reject":
        return "rejected"
    if d == "ask_for_explanation":
        return "awaiting_explanation"
    if required:
        return "pending_required_review"
    return "not_required"


def checkpoint_blocks(decision: str, required: bool, notes: str = "") -> bool:
    d = normalize_decision(decision)
    if d == "reject":
        return True
    if not required:
        return False
    return not decision_approved(d, notes)


def create_checkpoint(
    *,
    phase: str,
    artifact: dict[str, Any] | None = None,
    policy: str = "require_high_risk_only",
    decision: str | None = "pending",
    reviewer: str = "human",
    notes: str = "",
    run_id: str | None = None,
    required: bool | None = None,
) -> dict[str, Any]:
    if phase not in CHECKPOINT_PHASES:
        raise ValueError(f"Unsupported checkpoint phase: {phase}")
    artifact = artifact or {}
    policy = normalize_policy(policy)
    decision = normalize_decision(decision)
    is_required = phase_required(policy, phase, artifact, explicit_required=required)
    status = decision_status(decision, is_required, notes)
    approved = decision_approved(decision, notes) or (not is_required and decision != "reject")
    blocked = checkpoint_blocks(decision, is_required, notes)
    checkpoint = {
        "kind": "HumanCheckpoint",
        "run_id": run_id or "ad_hoc",
        "phase": phase,
        "label": PHASE_LABELS[phase],
        "question": PHASE_QUESTIONS[phase],
        "policy": policy,
        "required": is_required,
        "decision": decision,
        "status": status,
        "approved": approved,
        "blocked": blocked,
        "waived": decision == "skip" and bool(notes.strip()),
        "reviewer": reviewer,
        "notes": notes,
        "timestamp_ms": now_ms(),
        "artifact_digest": stable_digest(artifact),
        "artifact": artifact,
        "options": ["approve", "edit", "reject", "skip", "auto_approve", "ask_for_explanation"],
    }
    return checkpoint


def require_checkpoint_phase(phase: str, artifact: dict[str, Any], policy: str, *, run_id: str = "ad_hoc") -> dict[str, Any]:
    """Return a pending checkpoint for a phase based on policy."""

    return create_checkpoint(phase=phase, artifact=artifact, policy=policy, decision="pending", run_id=run_id)


def apply_checkpoint_decision(checkpoint: dict[str, Any], decision: str, *, reviewer: str | None = None, notes: str | None = None, artifact_patch: dict[str, Any] | None = None) -> dict[str, Any]:
    artifact = dict(checkpoint.get("artifact") or {})
    if artifact_patch:
        artifact.update(artifact_patch)
    return create_checkpoint(
        phase=str(checkpoint["phase"]),
        artifact=artifact,
        policy=str(checkpoint.get("policy") or "require_high_risk_only"),
        decision=decision,
        reviewer=reviewer or str(checkpoint.get("reviewer") or "human"),
        notes=notes if notes is not None else str(checkpoint.get("notes") or ""),
        run_id=str(checkpoint.get("run_id") or "ad_hoc"),
        required=bool(checkpoint.get("required", False)),
    )


def workflow_gate(checkpoints: Iterable[dict[str, Any]], *, policy: str = "require_high_risk_only", required_phases: Iterable[str] | None = None) -> dict[str, Any]:
    policy = normalize_policy(policy)
    checkpoint_list = list(checkpoints)
    by_phase: dict[str, list[dict[str, Any]]] = {}
    for checkpoint in checkpoint_list:
        by_phase.setdefault(str(checkpoint.get("phase")), []).append(checkpoint)

    required = list(required_phases or CHECKPOINT_PHASES)
    missing: list[str] = []
    pending: list[str] = []
    rejected: list[str] = []
    waived: list[str] = []
    approved: list[str] = []
    blocked: list[str] = []

    for phase in required:
        latest = by_phase.get(phase, [])[-1] if by_phase.get(phase) else None
        if latest is None:
            if policy == "require_all":
                missing.append(phase)
                blocked.append(phase)
            continue
        if latest.get("waived"):
            waived.append(phase)
        if latest.get("approved"):
            approved.append(phase)
        if latest.get("decision") == "reject":
            rejected.append(phase)
            blocked.append(phase)
        elif latest.get("blocked"):
            pending.append(phase)
            blocked.append(phase)

    can_continue = not blocked
    return {
        "kind": "HumanCheckpointGate",
        "policy": policy,
        "required_phases": required,
        "approved_phases": approved,
        "waived_phases": waived,
        "missing_phases": missing,
        "pending_phases": pending,
        "rejected_phases": rejected,
        "blocked_phases": blocked,
        "can_continue": can_continue,
        "production_status_allowed": can_continue,
        "checkpoint_count": len(checkpoint_list),
    }


def checkpoint_event(checkpoint: dict[str, Any]) -> dict[str, Any]:
    return {
        "ts_ms": checkpoint.get("timestamp_ms") or now_ms(),
        "state": "HumanCheckpointRecorded",
        "payload": checkpoint,
    }


def checkpoint_to_search_record(checkpoint: dict[str, Any]) -> dict[str, Any]:
    return {
        "doc_type": "human_checkpoint",
        "run_id": checkpoint.get("run_id"),
        "phase": checkpoint.get("phase"),
        "decision": checkpoint.get("decision"),
        "status": checkpoint.get("status"),
        "approved": bool(checkpoint.get("approved")),
        "blocked": bool(checkpoint.get("blocked")),
        "reviewer": checkpoint.get("reviewer"),
        "artifact_digest": checkpoint.get("artifact_digest"),
        "text": f"{checkpoint.get('label')}: {checkpoint.get('decision')} by {checkpoint.get('reviewer')}. {checkpoint.get('notes', '')}",
        "timestamp_ms": checkpoint.get("timestamp_ms"),
    }


def checkpoints_to_turtle(checkpoints: Iterable[dict[str, Any]], namespace: str = "https://github.com/daddydrac/veritas/ontology#") -> str:
    ns = Namespace(namespace if namespace.endswith(("#", "/")) else namespace + "#")
    graph = Graph()
    graph.bind("veritas", ns)
    for checkpoint in checkpoints:
        run_id = str(checkpoint.get("run_id") or "ad_hoc")
        phase = str(checkpoint.get("phase") or "unknown")
        digest = str(checkpoint.get("artifact_digest") or stable_digest(checkpoint))[:16]
        iri = ns[f"human_checkpoint_{safe_local_name(run_id)}_{safe_local_name(phase)}_{digest}"]
        graph.add((iri, RDF.type, ns.HumanCheckpoint))
        graph.add((iri, ns.hasIdentifier, Literal(f"{run_id}:{phase}:{digest}")))
        graph.add((iri, ns.hasCheckpointPhase, Literal(phase)))
        graph.add((iri, ns.hasHumanDecision, Literal(str(checkpoint.get("decision")))))
        graph.add((iri, ns.hasHumanValidationStatus, Literal(str(checkpoint.get("status")))))
        graph.add((iri, ns.hasHumanReviewer, Literal(str(checkpoint.get("reviewer")))))
        graph.add((iri, ns.isRequiredHumanCheckpoint, Literal(str(bool(checkpoint.get("required"))).lower())))
        graph.add((iri, ns.blocksWorkflowProgress, Literal(str(bool(checkpoint.get("blocked"))).lower())))
        graph.add((iri, ns.hasArtifactDigest, Literal(str(checkpoint.get("artifact_digest")))))
        if checkpoint.get("notes"):
            graph.add((iri, ns.hasDescription, Literal(str(checkpoint.get("notes")))))
    return graph.serialize(format="turtle")


def safe_local_name(value: str) -> str:
    s = "".join(ch if ch.isalnum() else "_" for ch in value)
    return s.strip("_") or "item"


def persist_human_workflow(workspace: Path, checkpoints: list[dict[str, Any]], gate: dict[str, Any], *, report_name: str = "human_workflow_report.json") -> dict[str, Any]:
    workspace.mkdir(parents=True, exist_ok=True)
    checkpoint_path = workspace / "human_checkpoints.jsonl"
    events_path = workspace / "events.jsonl"
    search_path = workspace / "human_checkpoint_search_records.jsonl"
    rdf_path = workspace / "human_checkpoints.ttl"
    report_path = workspace / report_name

    with checkpoint_path.open("w", encoding="utf-8") as fh:
        for checkpoint in checkpoints:
            fh.write(json.dumps(checkpoint, ensure_ascii=False) + "\n")
    with events_path.open("a", encoding="utf-8") as fh:
        for checkpoint in checkpoints:
            fh.write(json.dumps(checkpoint_event(checkpoint), ensure_ascii=False) + "\n")
    with search_path.open("w", encoding="utf-8") as fh:
        for checkpoint in checkpoints:
            fh.write(json.dumps(checkpoint_to_search_record(checkpoint), ensure_ascii=False) + "\n")
    turtle = checkpoints_to_turtle(checkpoints)
    rdf_path.write_text(turtle, encoding="utf-8")
    report = {
        "ok": bool(gate.get("can_continue")),
        "kind": "VeritasHumanReviewWorkflowReport",
        "human_checkpoints": checkpoints,
        "human_checkpoint_gate": gate,
        "rdf_path": str(rdf_path),
        "search_records_path": str(search_path),
        "events_path": str(events_path),
        "checkpoint_path": str(checkpoint_path),
    }
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    return report


def default_phase_artifacts() -> dict[str, dict[str, Any]]:
    return {
        "citation_review": {
            "apa_citation": "Doe, J. (2026). Representation-first symbolic shadows.",
            "source_url": "https://arxiv.org/abs/2601.00001",
            "citation_human_validated": False,
            "review_required": True,
        },
        "formula_review": {
            "formula_id": "eq-shadow-1",
            "latex": "E=mc^2",
            "normalized_latex": "E=mc^2",
            "formula_image_status": "rendered_mock",
            "latex_ocr_status": "ocr_complete",
            "confidence": 0.93,
            "human_validated": False,
            "use_for_codegen": True,
        },
        "representation_review": {
            "surface_phenomenon": "surface equation appears simple but may hide transformation constraints",
            "candidate_representation_map": "R: SurfaceEquation -> LatentTransformationConstraint",
            "invariants": ["energy equivalence under frame transformation"],
            "risks": ["high risk: formula is not a proof by itself"],
        },
        "plan_review": {
            "steps": ["retrieve evidence", "run SPARQL", "derive invariants", "generate code", "validate"],
            "risks": ["high risk: missing invariant test would invalidate production status"],
            "validation_gates": ["compile", "unit tests", "invariant assertion"],
        },
        "code_architecture_review": {
            "language": "rust",
            "files": ["Cargo.toml", "src/lib.rs"],
            "functional_constraints": ["pure domain core", "referential transparency", "side-effect boundary"],
            "commands": ["cargo test"],
            "risk": "high risk: generated files must remain inside workspace",
        },
        "validation_review": {
            "final_status_target": "production_candidate_validated",
            "commands_run": [{"command": "cargo test", "success": True}],
            "validation_results": [{"ok": True, "kind": "unit_tests"}],
            "risk": "high risk: validation approval required before production status",
        },
    }


def build_workflow_checkpoints(*, policy: str = "require_all", decisions: dict[str, str] | None = None, notes: dict[str, str] | None = None, run_id: str = "phase7-run", reviewer: str = "phase7-reviewer") -> list[dict[str, Any]]:
    decisions = decisions or {}
    notes = notes or {}
    artifacts = default_phase_artifacts()
    checkpoints: list[dict[str, Any]] = []
    for phase in CHECKPOINT_PHASES:
        checkpoint = create_checkpoint(
            phase=phase,
            artifact=artifacts[phase],
            policy=policy,
            decision=decisions.get(phase, "approve"),
            reviewer=reviewer,
            notes=notes.get(phase, "approved in source/mocked phase7 proof"),
            run_id=run_id,
        )
        checkpoints.append(checkpoint)
    return checkpoints


def source_mocked_phase7_summary(tmp_path: Path | None = None) -> dict[str, Any]:
    workspace = tmp_path or Path("data/e2e/source-mocked-human-workflow")
    workspace.mkdir(parents=True, exist_ok=True)

    approved_checkpoints = build_workflow_checkpoints(policy="require_all")
    approved_gate = workflow_gate(approved_checkpoints, policy="require_all")
    approved_report = persist_human_workflow(workspace, approved_checkpoints, approved_gate)

    missing_plan = [c for c in approved_checkpoints if c["phase"] != "plan_review"]
    missing_plan_gate = workflow_gate(missing_plan, policy="require_all")

    rejected_formula = build_workflow_checkpoints(policy="require_all", decisions={"formula_review": "reject"})
    rejected_formula_gate = workflow_gate(rejected_formula, policy="require_all")

    waived_plan = build_workflow_checkpoints(policy="require_all", decisions={"plan_review": "skip"}, notes={"plan_review": "Reviewed externally in design meeting VER-123."})
    waived_plan_gate = workflow_gate(waived_plan, policy="require_all")

    high_risk_auto = build_workflow_checkpoints(policy="require_high_risk_only", decisions={
        "citation_review": "auto_approve",
        "formula_review": "approve",
        "representation_review": "approve",
        "plan_review": "approve",
        "code_architecture_review": "approve",
        "validation_review": "approve",
    })
    high_risk_gate = workflow_gate(high_risk_auto, policy="require_high_risk_only")

    rdf_graph = Graph().parse(data=checkpoints_to_turtle(approved_checkpoints), format="turtle")
    search_records = [checkpoint_to_search_record(c) for c in approved_checkpoints]
    checks = [
        {"name": "require_all_approved_allows_progress", "ok": approved_gate["can_continue"] is True},
        {"name": "missing_required_plan_blocks_progress", "ok": "plan_review" in missing_plan_gate["missing_phases"] and missing_plan_gate["can_continue"] is False},
        {"name": "rejected_formula_blocks_codegen", "ok": "formula_review" in rejected_formula_gate["rejected_phases"] and rejected_formula_gate["can_continue"] is False},
        {"name": "explicit_waiver_allows_required_phase", "ok": waived_plan_gate["can_continue"] is True and "plan_review" in waived_plan_gate["waived_phases"]},
        {"name": "checkpoint_rdf_parseable", "ok": len(rdf_graph) > 0},
        {"name": "search_records_cover_all_phases", "ok": {r["phase"] for r in search_records} == set(CHECKPOINT_PHASES)},
        {"name": "persisted_report_contains_events_and_final_gate", "ok": Path(approved_report["events_path"]).exists() and approved_report["human_checkpoint_gate"]["production_status_allowed"] is True},
        {"name": "require_high_risk_policy_allows_low_risk_auto_citation", "ok": high_risk_gate["can_continue"] is True},
    ]
    summary = {
        "ok": all(c["ok"] for c in checks),
        "phase": "phase7_human_workflow",
        "checks": checks,
        "approved_gate": approved_gate,
        "missing_plan_gate": missing_plan_gate,
        "rejected_formula_gate": rejected_formula_gate,
        "waived_plan_gate": waived_plan_gate,
        "report_path": str(workspace / "human_workflow_report.json"),
        "rdf_triples": len(rdf_graph),
        "search_record_count": len(search_records),
    }
    (workspace / "phase7-summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    return summary
