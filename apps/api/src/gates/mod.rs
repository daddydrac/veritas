use crate::{
    append_jsonl, now_millis, write_json_file, ApiFailure, AppState,
};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) mod evidence;
pub(crate) mod human;
pub(crate) mod math_tools;
pub(crate) mod representation;
pub(crate) mod shacl;

const GATE_REPORT_FILE: &str = "pre_codegen_gate_report.json";
const GATE_DECISIONS_FILE: &str = "gate_decisions.jsonl";

pub(crate) async fn run_pre_codegen_gates(
    state: &AppState,
    workspace: &Path,
    run_id: &str,
    goal: &str,
    plan: &Value,
    automatic_shacl: &Value,
) -> Result<Value, ApiFailure> {
    let mut decisions: Vec<Value> = Vec::new();
    let mut blocked: Vec<Value> = Vec::new();

    let evidence_decision = evidence::evaluate(workspace).await;
    record_decision(workspace, &evidence_decision).await?;
    collect(&mut decisions, &mut blocked, evidence_decision);

    let representation_decision = representation::evaluate(workspace, goal, plan).await;
    record_decision(workspace, &representation_decision).await?;
    collect(&mut decisions, &mut blocked, representation_decision);

    let math_tools_decision = math_tools::evaluate(workspace, goal, plan).await;
    record_decision(workspace, &math_tools_decision).await?;
    collect(&mut decisions, &mut blocked, math_tools_decision);

    for phase in human::required_pre_codegen_checkpoint_phases() {
        let human_decision = human::evaluate_checkpoint(workspace, &state.human_loop_policy, &phase, plan).await;
        record_decision(workspace, &human_decision).await?;
        collect(&mut decisions, &mut blocked, human_decision);
    }

    let shacl_decision = shacl::evaluate_pre_codegen(automatic_shacl, &state.governance_mode);
    record_decision(workspace, &shacl_decision).await?;
    collect(&mut decisions, &mut blocked, shacl_decision);

    let report = json!({
        "ok": blocked.is_empty(),
        "kind": "VeritasPreCodegenGateReport",
        "run_id": run_id,
        "stage": "pre_codegen",
        "timestamp_ms": now_millis(),
        "policy": {
            "human_loop_policy": state.human_loop_policy.clone(),
            "governance_mode": state.governance_mode.as_str(),
            "shacl_enforced": state.governance_mode.enforces(),
            "required_human_checkpoints": human::required_pre_codegen_checkpoint_phases(),
        },
        "decisions": decisions,
        "blocked": blocked,
        "files_written_allowed": blocked.is_empty(),
        "commands_run_allowed": blocked.is_empty(),
    });
    write_json_file(&workspace.join(GATE_REPORT_FILE), &report).await?;

    if !blocked.is_empty() {
        return Err(ApiFailure::new(
            StatusCode::CONFLICT,
            "pre_codegen_gate.blocked",
            "Pre-codegen gates blocked execution before generated files or validation commands could run.",
            "Inspect pre_codegen_gate_report.json and gate_decisions.jsonl, complete the required evidence/human/representation/math/SHACL actions, then resume the journey."
        ).with_details(report));
    }

    Ok(report)
}

async fn record_decision(workspace: &Path, decision: &Value) -> Result<(), ApiFailure> {
    append_jsonl(&workspace.join(GATE_DECISIONS_FILE), decision).await
}

fn collect(decisions: &mut Vec<Value>, blocked: &mut Vec<Value>, decision: Value) {
    if decision.get("blocking").and_then(Value::as_bool).unwrap_or(false)
        || decision.get("ok").and_then(Value::as_bool) == Some(false)
            && decision.get("enforced").and_then(Value::as_bool).unwrap_or(true)
    {
        blocked.push(decision.clone());
    }
    decisions.push(decision);
}

pub(crate) async fn write_pre_codegen_blocked_report(
    state: &AppState,
    workspace: &Path,
    run_id: &str,
    goal: &str,
    language: &str,
    plan: &Value,
    gate_error: ApiFailure,
) -> Result<Value, ApiFailure> {
    let gate_details = gate_error.details.clone();
    let blocked_stage = first_blocked_stage(&gate_details).unwrap_or_else(|| "pre_codegen_gate".to_string());
    let artifact_decision = crate::artifact_decision::decide_blocked_pre_codegen(
        state,
        workspace,
        run_id,
        goal,
        language,
        &gate_details,
    ).await?;
    let status = artifact_decision.get("artifact_status").and_then(Value::as_str).unwrap_or("failed").to_string();
    let human_checkpoints = crate::read_events_tail(&workspace.join("human_checkpoints.jsonl"), 500).await.unwrap_or_default();
    let human_checkpoint_gate = crate::human_checkpoint_gate_summary(workspace, &state.human_loop_policy).await;
    let plan_envelope = crate::read_json_file(&workspace.join("plan_envelope.json")).await.unwrap_or_else(|| json!({"plan": plan}));
    let empty_code_package = json!({});
    let empty_commands: Vec<Value> = Vec::new();
    let empty_validation: Vec<Value> = Vec::new();
    let empty_repairs: Vec<Value> = Vec::new();
    let report_lineage = crate::lineage::build_report_lineage(workspace, &plan_envelope, plan, &empty_code_package, &empty_commands, &empty_validation, &empty_repairs, &artifact_decision).await?;
    let report = json!({
        "ok": false,
        "kind": "VeritasAutonomousRunReport",
        "run_id": run_id,
        "workspace": workspace.display().to_string(),
        "original_task": goal,
        "language": language,
        "source_documents": report_lineage.get("source_documents").cloned().unwrap_or_else(|| json!({})),
        "citations": report_lineage.get("citations").cloned().unwrap_or_else(|| json!({})),
        "formulas": report_lineage.get("formulas").cloned().unwrap_or_else(|| json!({})),
        "review_decisions": report_lineage.get("review_decisions").cloned().unwrap_or_else(|| json!({})),
        "representation_model": report_lineage.get("representation_model").cloned().unwrap_or_else(|| json!({})),
        "planning_context": report_lineage.get("planning_context").cloned().unwrap_or_else(|| json!({})),
        "generated_plan": plan,
        "plan_lineage": report_lineage.get("plan_lineage").cloned().unwrap_or_else(|| json!({})),
        "file_lineage": report_lineage.get("file_lineage").cloned().unwrap_or_else(|| json!([])),
        "command_lineage": report_lineage.get("command_lineage").cloned().unwrap_or_else(|| json!([])),
        "validation_lineage": report_lineage.get("validation_lineage").cloned().unwrap_or_else(|| json!([])),
        "repair_lineage": report_lineage.get("repair_lineage").cloned().unwrap_or_else(|| json!([])),
        "governance_lineage": report_lineage.get("governance_lineage").cloned().unwrap_or_else(|| json!({})),
        "model_routes_used": {"planner": crate::role_json(&state.planner_model), "code": crate::role_json(&state.code_model), "math": crate::role_json(&state.math_model)},
        "tool_calls_performed": [],
        "human_checkpoint_policy": state.human_loop_policy.clone(),
        "human_checkpoints": human_checkpoints,
        "human_checkpoint_gate": human_checkpoint_gate,
        "pre_codegen_gate": gate_details,
        "artifact_decision": artifact_decision,
        "files_changed": [],
        "commands_run": [],
        "validation_results": [],
        "attempts_performed": 0,
        "retries_performed": 0,
        "retry_history": [],
        "generated_package_status": status.clone(),
        "artifact_status": status.clone(),
        "final_status": status.clone(),
        "blocked_stage": blocked_stage.clone(),
        "remaining_limitations": ["Pre-codegen gates blocked execution before code generation. Resolve the listed gate decisions and resume."],
        "code_model_output": {},
        "artifact_lifecycle_state": status.clone(),
        "state": status,
    });
    write_json_file(&workspace.join("pre_codegen_blocked_report.json"), &report).await?;
    write_json_file(&workspace.join("final_report.json"), &report).await?;
    crate::persist_run_state(workspace, "PreCodegenBlocked", json!({"final_status": report.get("final_status"), "artifact_decision": "artifact_decision.json", "blocked_stage": blocked_stage.clone(), "report": "final_report.json"})).await?;
    Ok(report)
}

fn first_blocked_stage(details: &Value) -> Option<String> {
    details.get("blocked")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("stage").or_else(|| item.get("gate")))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| details.get("blocked_stage").and_then(Value::as_str).map(ToString::to_string))
}

fn status_for_stage(stage: &str) -> &'static str {
    match stage {
        "evidence_eligibility" => "awaiting_evidence_review",
        "plan_review" | "code_architecture_review" | "validation_review" => "awaiting_human_approval",
        "representation_gate" => "awaiting_representation_review",
        "math_tools_gate" => "blocked_by_math_tools",
        "pre_codegen_shacl" => "blocked_by_governance",
        _ => "blocked_by_pre_codegen_gate",
    }
}
