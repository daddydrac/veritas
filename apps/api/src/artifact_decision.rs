use crate::{read_events_tail, read_json_file, shacl_report_conforms, write_json_file, ApiFailure, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, env, path::Path};

const ARTIFACT_DECISION_FILE: &str = "artifact_decision.json";
const HOST_VALIDATION_JSONL: &str = "host_validation_summary.jsonl";
const HOST_VALIDATION_JSON: &str = "host_validation_summary.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArtifactStatus {
    Draft,
    IngestionIncomplete,
    AwaitingEvidenceReview,
    AwaitingHumanApproval,
    BlockedByFormulaReview,
    BlockedByCitationReview,
    BlockedByMathTools,
    BlockedByGovernance,
    ValidationFailed,
    RepairFailed,
    LocalValidatedHostPending,
    DevOnlyUnverified,
    ProductionCandidateValidated,
    ProductionValidated,
    Cancelled,
    Failed,
}

impl ArtifactStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::IngestionIncomplete => "ingestion_incomplete",
            Self::AwaitingEvidenceReview => "awaiting_evidence_review",
            Self::AwaitingHumanApproval => "awaiting_human_approval",
            Self::BlockedByFormulaReview => "blocked_by_formula_review",
            Self::BlockedByCitationReview => "blocked_by_citation_review",
            Self::BlockedByMathTools => "blocked_by_math_tools",
            Self::BlockedByGovernance => "blocked_by_governance",
            Self::ValidationFailed => "validation_failed",
            Self::RepairFailed => "repair_failed",
            Self::LocalValidatedHostPending => "local_validated_host_pending",
            Self::DevOnlyUnverified => "dev_only_unverified",
            Self::ProductionCandidateValidated => "production_candidate_validated",
            Self::ProductionValidated => "production_validated",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn production_allowed(&self) -> bool {
        matches!(self, Self::ProductionCandidateValidated | Self::ProductionValidated)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HostValidationStatus {
    NotRequired,
    Pending,
    Passed,
    Failed,
}

impl HostValidationStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Pending => "host_validation_pending",
            Self::Passed => "host_validation_passed",
            Self::Failed => "host_validation_failed",
        }
    }
}

pub(crate) async fn decide_completed_run(
    state: &AppState,
    workspace: &Path,
    run_id: &str,
    validation_runtime_status: &str,
    pre_codegen_gates: &Value,
    final_shacl_report: &Value,
    human_checkpoint_gate: &Value,
    validation_results: &[Value],
    retry_history: &[Value],
    commands_run: &[Value],
    files_changed: &[String],
    cancelled: bool,
) -> Result<Value, ApiFailure> {
    let gate_decisions = read_events_tail(&workspace.join("gate_decisions.jsonl"), 2000).await.unwrap_or_default();
    let evidence_eligibility = read_json_file(&workspace.join("evidence_eligibility.json")).await;
    let planning_context = read_json_file(&workspace.join("planning_context.json")).await;
    let host_validation_events = read_host_validation_events(workspace).await;
    let host_status = classify_host_validation(&host_validation_events);
    let validation_status = classify_validation(validation_runtime_status, validation_results, commands_run, cancelled);
    let shacl_conforms = shacl_report_conforms(final_shacl_report);
    let human_allowed = human_checkpoint_gate
        .get("production_status_allowed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let pre_codegen_ok = pre_codegen_gates.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let mut factors = BTreeMap::<String, Value>::new();
    factors.insert("pre_codegen_gates".to_string(), json!({
        "ok": pre_codegen_ok,
        "blocked": pre_codegen_gates.get("blocked").cloned().unwrap_or_else(|| json!([])),
    }));
    factors.insert("validation".to_string(), validation_status.clone());
    factors.insert("human_checkpoint_gate".to_string(), human_checkpoint_gate.clone());
    factors.insert("final_shacl".to_string(), json!({
        "conforms": shacl_conforms,
        "governance_mode": state.governance_mode.as_str(),
        "enforced": state.governance_mode.enforces(),
    }));
    factors.insert("host_validation".to_string(), json!({
        "status": host_status.as_str(),
        "events": host_validation_events,
    }));
    if let Some(value) = evidence_eligibility.clone() {
        factors.insert("evidence_eligibility".to_string(), value);
    }
    if let Some(value) = planning_context.clone() {
        factors.insert("planning_context".to_string(), value);
    }

    let mut limitations: Vec<String> = Vec::new();
    let mut status = ArtifactStatus::Draft;
    let mut reason = String::new();

    if cancelled || validation_runtime_status == "cancelled" {
        status = ArtifactStatus::Cancelled;
        reason = "Run was cancelled before a validated artifact decision could be produced.".to_string();
    } else if !pre_codegen_ok {
        status = status_from_gate_failures(&gate_decisions, pre_codegen_gates, evidence_eligibility.as_ref());
        reason = "Pre-codegen gates did not pass; no production artifact can be created.".to_string();
    } else if state.governance_mode.enforces() && !shacl_conforms {
        status = ArtifactStatus::BlockedByGovernance;
        reason = "Final SHACL governance did not conform in enforce mode.".to_string();
    } else if !human_allowed {
        status = ArtifactStatus::AwaitingHumanApproval;
        reason = "Required human approval gate did not allow production status.".to_string();
    } else if validation_status.get("passed").and_then(Value::as_bool) != Some(true) {
        if !retry_history.is_empty() {
            status = ArtifactStatus::RepairFailed;
            reason = "Validation failed after one or more repair attempts.".to_string();
        } else {
            status = ArtifactStatus::ValidationFailed;
            reason = "Compile/test validation did not pass.".to_string();
        }
    } else if planning_context.as_ref().and_then(|value| value.get("status")).and_then(Value::as_str) == Some("dev_only_unverified") {
        status = ArtifactStatus::DevOnlyUnverified;
        reason = "Planning used dev_exploratory evidence bypass; artifact is explicitly non-production and cannot be production validated.".to_string();
    } else if matches!(host_status, HostValidationStatus::Failed) {
        status = ArtifactStatus::Failed;
        reason = "Host validation reported a failure.".to_string();
    } else if matches!(host_status, HostValidationStatus::Passed) {
        status = ArtifactStatus::ProductionValidated;
        reason = "All application gates, validation checks, governance checks, and host validation checks passed.".to_string();
    } else if candidate_without_host_allowed() {
        status = ArtifactStatus::ProductionCandidateValidated;
        reason = "Application gates and validation passed; host production validation remains pending by configured candidate policy.".to_string();
    } else {
        status = ArtifactStatus::LocalValidatedHostPending;
        reason = "Application gates and validation passed locally, but host production validation is pending.".to_string();
    }

    collect_limitations(&mut limitations, &status, &reason, &host_status, state.governance_mode.as_str(), files_changed, commands_run);

    let decision = json!({
        "ok": matches!(status, ArtifactStatus::LocalValidatedHostPending | ArtifactStatus::ProductionCandidateValidated | ArtifactStatus::ProductionValidated),
        "kind": "VeritasArtifactDecision",
        "run_id": run_id,
        "artifact_status": status.as_str(),
        "final_status": status.as_str(),
        "validation_status": validation_status,
        "production_status_allowed": status.production_allowed(),
        "host_validation_status": host_status.as_str(),
        "governance_mode": state.governance_mode.as_str(),
        "decision_reason": reason,
        "decision_factors": factors,
        "remaining_limitations": limitations,
        "files_changed_count": files_changed.len(),
        "commands_run_count": commands_run.len(),
        "decision_source": "application_artifact_decision_engine",
    });
    write_json_file(&workspace.join(ARTIFACT_DECISION_FILE), &decision).await?;
    Ok(decision)
}

pub(crate) async fn decide_blocked_pre_codegen(
    state: &AppState,
    workspace: &Path,
    run_id: &str,
    goal: &str,
    language: &str,
    gate_details: &Value,
) -> Result<Value, ApiFailure> {
    let gate_decisions = read_events_tail(&workspace.join("gate_decisions.jsonl"), 2000).await.unwrap_or_default();
    let evidence_eligibility = read_json_file(&workspace.join("evidence_eligibility.json")).await;
    let status = status_from_gate_failures(&gate_decisions, gate_details, evidence_eligibility.as_ref());
    let reason = format!("Pre-codegen gate blocked execution at status {}.", status.as_str());
    let decision = json!({
        "ok": false,
        "kind": "VeritasArtifactDecision",
        "run_id": run_id,
        "artifact_status": status.as_str(),
        "final_status": status.as_str(),
        "validation_status": {"passed": false, "status": "not_run", "reason": "Pre-codegen gates blocked validation before commands could run."},
        "production_status_allowed": false,
        "host_validation_status": "not_run",
        "governance_mode": state.governance_mode.as_str(),
        "decision_reason": reason,
        "decision_factors": {
            "goal": goal,
            "language": language,
            "pre_codegen_gate": gate_details,
            "gate_decisions": gate_decisions,
            "evidence_eligibility": evidence_eligibility,
        },
        "remaining_limitations": ["Pre-codegen gates blocked execution before code generation. Resolve the listed gate decisions and resume."],
        "files_changed_count": 0,
        "commands_run_count": 0,
        "decision_source": "application_artifact_decision_engine",
    });
    write_json_file(&workspace.join(ARTIFACT_DECISION_FILE), &decision).await?;
    Ok(decision)
}

fn status_from_gate_failures(gate_decisions: &[Value], gate_details: &Value, evidence_eligibility: Option<&Value>) -> ArtifactStatus {
    if evidence_has_blocked_formulas(evidence_eligibility) {
        return ArtifactStatus::BlockedByFormulaReview;
    }
    if evidence_has_blocked_citations(evidence_eligibility) {
        return ArtifactStatus::BlockedByCitationReview;
    }
    let combined = json!({"decisions": gate_decisions, "details": gate_details});
    let text = combined.to_string().to_ascii_lowercase();
    if text.contains("blocked_by_formula_review") || text.contains("not_eligible") || text.contains("formula") && text.contains("rejected") {
        return ArtifactStatus::BlockedByFormulaReview;
    }
    if text.contains("blocked_by_citation_review") || text.contains("citation") && (text.contains("rejected") || text.contains("pending_review")) {
        return ArtifactStatus::BlockedByCitationReview;
    }
    if text.contains("awaiting_evidence_review") || text.contains("evidence_eligibility") {
        return ArtifactStatus::AwaitingEvidenceReview;
    }
    if text.contains("awaiting_human_approval") || text.contains("plan_review") || text.contains("code_architecture_review") || text.contains("validation_review") {
        return ArtifactStatus::AwaitingHumanApproval;
    }
    if text.contains("blocked_by_math_tools") || text.contains("math_tools_gate") {
        return ArtifactStatus::BlockedByMathTools;
    }
    if text.contains("blocked_by_governance") || text.contains("shacl") || text.contains("representation_gate") {
        return ArtifactStatus::BlockedByGovernance;
    }
    ArtifactStatus::Failed
}

fn evidence_has_blocked_formulas(evidence: Option<&Value>) -> bool {
    evidence
        .and_then(|value| value.get("formula_to_code"))
        .and_then(Value::as_object)
        .map(|obj| obj.values().any(|item| item.get("ok").and_then(Value::as_bool) == Some(false)))
        .unwrap_or(false)
        || evidence
            .and_then(|value| value.get("blocked_formulas"))
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false)
}

fn evidence_has_blocked_citations(evidence: Option<&Value>) -> bool {
    evidence
        .and_then(|value| value.get("blocked_citations"))
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
        || evidence
            .and_then(|value| value.get("approved_citations"))
            .and_then(Value::as_array)
            .map(|items| items.is_empty())
            .unwrap_or(false)
}

fn classify_validation(validation_runtime_status: &str, validation_results: &[Value], commands_run: &[Value], cancelled: bool) -> Value {
    if cancelled || validation_runtime_status == "cancelled" {
        return json!({"passed": false, "status": "cancelled"});
    }
    let commands_all_passed = !commands_run.is_empty() && commands_run.iter().all(|item| item.get("success").and_then(Value::as_bool).unwrap_or(false));
    let attempts = validation_results.len();
    let any_failure = commands_run.iter().any(|item| item.get("success").and_then(Value::as_bool) == Some(false));
    json!({
        "passed": commands_all_passed,
        "status": if commands_all_passed { "passed" } else if any_failure { "failed" } else { "not_run" },
        "attempts": attempts,
        "commands_run": commands_run.len(),
        "runtime_status_before_decision": validation_runtime_status,
    })
}

async fn read_host_validation_events(workspace: &Path) -> Vec<Value> {
    if let Ok(events) = read_events_tail(&workspace.join(HOST_VALIDATION_JSONL), 2000).await {
        if !events.is_empty() {
            return events;
        }
    }
    if let Some(value) = read_json_file(&workspace.join(HOST_VALIDATION_JSON)).await {
        return value.as_array().cloned().unwrap_or_else(|| vec![value]);
    }
    Vec::new()
}

fn classify_host_validation(events: &[Value]) -> HostValidationStatus {
    if events.is_empty() {
        return HostValidationStatus::Pending;
    }
    let mut saw_required = false;
    for item in events {
        let required = item.get("required").and_then(Value::as_bool).unwrap_or(true);
        if required {
            saw_required = true;
        }
        let ok = item.get("ok").or_else(|| item.get("success")).and_then(Value::as_bool);
        let status = item.get("status").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        if required && (ok == Some(false) || status.contains("fail") || status.contains("error")) {
            return HostValidationStatus::Failed;
        }
        if required && (ok.is_none() && status.contains("pending")) {
            return HostValidationStatus::Pending;
        }
    }
    if saw_required { HostValidationStatus::Passed } else { HostValidationStatus::NotRequired }
}

fn candidate_without_host_allowed() -> bool {
    env::var("VERITAS_ALLOW_PRODUCTION_CANDIDATE_WITH_HOST_PENDING")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn collect_limitations(
    limitations: &mut Vec<String>,
    status: &ArtifactStatus,
    reason: &str,
    host_status: &HostValidationStatus,
    governance_mode: &str,
    files_changed: &[String],
    commands_run: &[Value],
) {
    if !reason.trim().is_empty() {
        limitations.push(reason.to_string());
    }
    if matches!(host_status, HostValidationStatus::Pending) {
        limitations.push("Host validation has not passed; production_validated is not allowed.".to_string());
    }
    if governance_mode == "disabled" {
        limitations.push("Governance is disabled; production readiness cannot be claimed.".to_string());
    }
    if files_changed.is_empty() && !matches!(status, ArtifactStatus::AwaitingEvidenceReview | ArtifactStatus::AwaitingHumanApproval | ArtifactStatus::BlockedByFormulaReview | ArtifactStatus::BlockedByCitationReview | ArtifactStatus::BlockedByMathTools | ArtifactStatus::BlockedByGovernance) {
        limitations.push("No generated files were recorded.".to_string());
    }
    if commands_run.is_empty() && matches!(status, ArtifactStatus::LocalValidatedHostPending | ArtifactStatus::ProductionCandidateValidated | ArtifactStatus::ProductionValidated) {
        limitations.push("No validation commands were recorded, so production validation cannot be complete.".to_string());
    }
    limitations.sort();
    limitations.dedup();
}
