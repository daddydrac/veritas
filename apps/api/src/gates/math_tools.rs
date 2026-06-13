use serde_json::{json, Value};
use std::path::Path;

pub(crate) async fn evaluate(workspace: &Path, goal: &str, plan: &Value) -> Value {
    let math_heavy = is_math_heavy(workspace, goal, plan).await;
    if !math_heavy {
        return json!({
            "ok": true,
            "enforced": false,
            "blocking": false,
            "gate": "math_tools",
            "stage": "math_tools_gate",
            "status": "not_applicable_non_math_run",
            "files_written_allowed": true,
            "commands_run_allowed": true,
        });
    }
    let report_path = workspace.join("math_validation_report.json");
    let Some(report) = crate::read_json_file(&report_path).await else {
        return json!({
            "ok": false,
            "enforced": true,
            "blocking": true,
            "gate": "math_tools",
            "stage": "math_tools_gate",
            "status": "blocked_by_math_tools",
            "reason": "Math-heavy execution requires math_validation_report.json produced by the Tool-Verified Math Engine before code generation.",
            "math_validation_report_path": report_path.display().to_string(),
            "next_action": "Run the Tool-Verified Math Engine for eligible formulas, then resume. This gate rejects LLM-only mathematical claims.",
            "files_written_allowed": false,
            "commands_run_allowed": false,
        });
    };

    let report_ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let counterexamples = report.get("counterexamples").and_then(Value::as_array).cloned().unwrap_or_default();
    let blocking_findings = report.get("blocking_findings").and_then(Value::as_array).cloned().unwrap_or_default();
    let tool_results = report.get("tool_results").and_then(Value::as_array).cloned().unwrap_or_default();
    let status = report.get("status").and_then(Value::as_str).unwrap_or("unknown");
    let ok = report_ok && counterexamples.is_empty() && blocking_findings.is_empty() && !tool_results.is_empty();
    json!({
        "ok": ok,
        "enforced": true,
        "blocking": !ok,
        "gate": "math_tools",
        "stage": "math_tools_gate",
        "status": if ok { "math_tools_validated" } else { "blocked_by_math_tools" },
        "math_validation_status": status,
        "math_validation_report_path": report_path.display().to_string(),
        "tool_results_count": tool_results.len(),
        "counterexamples_count": counterexamples.len(),
        "blocking_findings_count": blocking_findings.len(),
        "report": report,
        "files_written_allowed": ok,
        "commands_run_allowed": ok,
    })
}

async fn is_math_heavy(workspace: &Path, goal: &str, plan: &Value) -> bool {
    if workspace.join("formula_manifest.json").exists() || workspace.join("formulas.jsonl").exists() || workspace.join("evidence_registry.json").exists() {
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
