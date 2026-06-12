use serde_json::{json, Value};
use std::path::Path;

const READY_STATUSES: &[&str] = &["approved", "accepted", "ready", "waived", "approved_or_waived"];

pub(crate) async fn evaluate(workspace: &Path, goal: &str, plan: &Value) -> Value {
    let math_heavy = is_math_heavy(workspace, goal, plan).await;
    if !math_heavy {
        return json!({
            "ok": true,
            "enforced": false,
            "blocking": false,
            "gate": "representation",
            "stage": "representation_gate",
            "status": "not_applicable_non_math_run",
            "files_written_allowed": true,
            "commands_run_allowed": true,
        });
    }

    let path = workspace.join("representation_model.json");
    let Some(model) = crate::read_json_file(&path).await else {
        return json!({
            "ok": false,
            "enforced": true,
            "blocking": true,
            "gate": "representation",
            "stage": "representation_gate",
            "status": "awaiting_representation_review",
            "reason": "Math-heavy execution requires representation_model.json before code generation.",
            "representation_model_path": path.display().to_string(),
            "next_action": "Create/approve a representation model containing surface phenomenon, representation map, invariants, symbolic shadows, and validation obligations.",
            "files_written_allowed": false,
            "commands_run_allowed": false,
        });
    };

    let status = model.get("status").and_then(Value::as_str).unwrap_or("pending").to_ascii_lowercase();
    let ready_status = READY_STATUSES.contains(&status.as_str());
    let invariants_ok = non_empty_array(&model, "invariants") || non_empty_array(&model, "candidate_invariants");
    let symbolic_ok = non_empty_array(&model, "symbolic_shadows") || model.get("symbolic_shadow").is_some();
    let validation_ok = non_empty_array(&model, "validation_obligations") || non_empty_array(&model, "validation_requirements");
    let representation_ok = model.get("representation_map").is_some() || model.get("candidate_representation_map").is_some();
    let missing = missing_fields(&[
        ("approved status", ready_status),
        ("representation_map", representation_ok),
        ("invariants", invariants_ok),
        ("symbolic_shadows", symbolic_ok),
        ("validation_obligations", validation_ok),
    ]);
    let ok = missing.is_empty();
    json!({
        "ok": ok,
        "enforced": true,
        "blocking": !ok,
        "gate": "representation",
        "stage": "representation_gate",
        "status": if ok { "representation_ready" } else { "awaiting_representation_review" },
        "representation_model_path": path.display().to_string(),
        "missing": missing,
        "model": model,
        "files_written_allowed": ok,
        "commands_run_allowed": ok,
    })
}

async fn is_math_heavy(workspace: &Path, goal: &str, plan: &Value) -> bool {
    if workspace.join("formula_manifest.json").exists() || workspace.join("formulas.jsonl").exists() {
        return true;
    }
    if plan.get("symbolic_shadows").is_some() || plan.get("math_readiness").is_some() {
        return true;
    }
    if plan.get("steps").and_then(Value::as_array).map(|steps| steps.iter().any(|step| step.get("tool").and_then(Value::as_str) == Some("math_reasoning"))).unwrap_or(false) {
        return true;
    }
    let lower = goal.to_ascii_lowercase();
    ["formula", "theorem", "latex", "invariant", "symbolic", "equation", "proof", "gradient", "matrix", "tensor", "derivative", "integral"].iter().any(|needle| lower.contains(needle))
}

fn non_empty_array(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_array).map(|items| !items.is_empty()).unwrap_or(false)
}

fn missing_fields(fields: &[(&str, bool)]) -> Vec<String> {
    fields.iter().filter_map(|(name, ok)| if *ok { None } else { Some((*name).to_string()) }).collect()
}
