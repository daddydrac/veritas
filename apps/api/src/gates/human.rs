use serde_json::{json, Value};
use std::{collections::HashMap, env, path::Path};

const DEFAULT_PRE_CODEGEN_CHECKPOINTS: &[&str] = &["plan_review", "code_architecture_review"];

pub(crate) fn required_pre_codegen_checkpoint_phases() -> Vec<String> {
    let configured = env::var("VERITAS_PRE_CODEGEN_CHECKPOINTS").unwrap_or_else(|_| DEFAULT_PRE_CODEGEN_CHECKPOINTS.join(","));
    let mut phases: Vec<String> = configured
        .split(',')
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect();
    phases.sort();
    phases.dedup();
    phases
}

pub(crate) async fn evaluate_checkpoint(workspace: &Path, policy: &str, phase: &str, artifact: &Value) -> Value {
    let checkpoints = crate::read_events_tail(&workspace.join("human_checkpoints.jsonl"), 1000).await.unwrap_or_default();
    let mut latest: HashMap<String, Value> = HashMap::new();
    for checkpoint in checkpoints {
        if let Some(checkpoint_phase) = checkpoint.get("phase").and_then(Value::as_str) {
            latest.insert(checkpoint_phase.to_ascii_lowercase(), checkpoint);
        }
    }
    let required = crate::human_checkpoint_required(policy, phase, artifact);
    if !required {
        return json!({
            "ok": true,
            "enforced": false,
            "blocking": false,
            "gate": "human_checkpoint",
            "stage": phase,
            "phase": phase,
            "status": "not_required_by_policy",
            "policy": policy,
            "required": false,
            "files_written_allowed": true,
            "commands_run_allowed": true,
        });
    }

    match latest.get(phase) {
        Some(checkpoint) => {
            let decision = checkpoint.get("decision").and_then(Value::as_str).unwrap_or("pending").to_ascii_lowercase();
            let notes = checkpoint.get("notes").and_then(Value::as_str).unwrap_or("");
            let approved = checkpoint.get("approved").and_then(Value::as_bool).unwrap_or_else(|| crate::human_decision_approved(&decision, notes));
            let blocked = checkpoint.get("blocked").and_then(Value::as_bool).unwrap_or_else(|| crate::human_decision_blocks(&decision, required, notes));
            let ok = approved && !blocked;
            json!({
                "ok": ok,
                "enforced": true,
                "blocking": !ok,
                "gate": "human_checkpoint",
                "stage": phase,
                "phase": phase,
                "policy": policy,
                "required": true,
                "decision": decision,
                "checkpoint": checkpoint,
                "status": if ok { "approved_or_waived" } else { "awaiting_human_approval" },
                "next_action": if ok { "Continue." } else { "Record an approve/edit/skip-with-waiver decision for this phase before code generation." },
                "files_written_allowed": ok,
                "commands_run_allowed": ok,
            })
        }
        None => json!({
            "ok": false,
            "enforced": true,
            "blocking": true,
            "gate": "human_checkpoint",
            "stage": phase,
            "phase": phase,
            "policy": policy,
            "required": true,
            "status": "awaiting_human_approval",
            "reason": "Required pre-codegen human checkpoint is missing.",
            "next_action": format!("Run `veritas journey review <run_id> --phase {phase} --decision approve` or provide an explicit waiver with notes."),
            "files_written_allowed": false,
            "commands_run_allowed": false,
        }),
    }
}
