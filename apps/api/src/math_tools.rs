use crate::{append_jsonl, read_json_file, write_json_file, ApiFailure, AppState};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::time::timeout;

const REPORT_FILE: &str = "math_validation_report.json";
const CALLS_FILE: &str = "math_tool_calls.jsonl";
const RESULTS_FILE: &str = "math_tool_results.jsonl";
const DEFAULT_TOOL_SEQUENCE: &[&str] = &[
    "parse_latex",
    "normalize_expression",
    "symbolic_simplify",
    "numeric_validate",
    "counterexample_search",
    "dimension_check",
    "generate_property_tests",
];

#[derive(Debug, Clone)]
struct FormulaCandidate {
    id: String,
    latex: String,
    normalized_latex: String,
    assumptions: Vec<String>,
    variables: Vec<String>,
    metadata: Value,
}

pub(crate) async fn status(state: &AppState) -> Result<Value, ApiFailure> {
    let url = format!("{}/health", state.math_tools_url.trim_end_matches('/'));
    let response = timeout(Duration::from_secs(state.math_tools_timeout_secs), state.http.get(&url).send()).await
        .map_err(|_| ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.timeout", format!("Math tools health check timed out after {}s", state.math_tools_timeout_secs), "Start the math-tools service or increase VERITAS_MATH_TOOLS_TIMEOUT_SECS."))?
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.transport", format!("Could not reach math tools service: {error}"), "Start services/math_tools or set VERITAS_MATH_TOOLS_URL to a reachable service."))?;
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or_else(|_| json!({"ok": false, "error": "health body was not valid JSON"}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.unhealthy", format!("Math tools service returned HTTP {}", status.as_u16()), "Inspect math-tools logs and dependencies.").with_details(body));
    }
    Ok(body)
}

pub(crate) async fn validate_workspace_if_required(state: &AppState, workspace: &Path, goal: &str, plan: &Value) -> Result<Option<Value>, ApiFailure> {
    if !math_heavy(workspace, goal, plan).await {
        return Ok(None);
    }
    let report_path = workspace.join(REPORT_FILE);
    if let Some(existing) = read_json_file(&report_path).await {
        return Ok(Some(existing));
    }
    let formulas = collect_formula_candidates(workspace).await?;
    let report = if formulas.is_empty() {
        json!({
            "kind": "VeritasMathValidationReport",
            "ok": false,
            "status": "failed_no_formula_candidates",
            "formula_count": 0,
            "tool_results": [],
            "blocking_findings": [{"code": "math_tools.no_formulas", "message": "Math-heavy run has no formula candidates available to validate."}],
            "counterexamples": [],
            "service_url": state.math_tools_url.clone(),
        })
    } else {
        validate_formulas(state, workspace, &formulas).await?
    };
    write_json_file(&report_path, &report).await?;
    Ok(Some(report))
}

pub(crate) async fn validate_formula_context(state: &AppState, formula_context: &Value) -> Result<Value, ApiFailure> {
    let formula = formula_context.get("formula_latex")
        .or_else(|| formula_context.get("normalized_latex"))
        .or_else(|| formula_context.get("raw_latex"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if formula.is_empty() {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "math_tools.formula_missing", "Formula context did not contain formula_latex/raw_latex/normalized_latex.", "Provide an approved formula record or formula_latex."));
    }
    let candidate = FormulaCandidate {
        id: formula_context.get("formula_id").and_then(Value::as_str).unwrap_or("ad_hoc_formula").to_string(),
        normalized_latex: formula.clone(),
        latex: formula,
        assumptions: string_array(formula_context.get("assumptions")),
        variables: string_array(formula_context.get("variables")),
        metadata: formula_context.clone(),
    };
    validate_formula_list_without_workspace(state, &[candidate]).await
}

async fn validate_formulas(state: &AppState, workspace: &Path, formulas: &[FormulaCandidate]) -> Result<Value, ApiFailure> {
    let mut tool_results = Vec::new();
    let mut blocking_findings = Vec::new();
    let mut counterexamples = Vec::new();
    for formula in formulas {
        let results = validate_single_formula(state, formula).await;
        for result in results {
            let _ = append_jsonl(&workspace.join(CALLS_FILE), &json!({
                "tool_call_id": result.get("tool_call_id"),
                "tool_name": result.get("tool_name"),
                "formula_id": formula.id,
                "input_hash": result.get("input_hash"),
            })).await;
            let _ = append_jsonl(&workspace.join(RESULTS_FILE), &result).await;
            if !result.get("ok").and_then(Value::as_bool).unwrap_or(false)
                && result.get("blocks_codegen").and_then(Value::as_bool).unwrap_or(true)
            {
                blocking_findings.push(json!({
                    "formula_id": formula.id,
                    "tool_name": result.get("tool_name"),
                    "result": result,
                }));
            }
            if let Some(items) = result.pointer("/result/counterexamples").and_then(Value::as_array) {
                for item in items {
                    counterexamples.push(json!({"formula_id": formula.id, "tool_name": result.get("tool_name"), "counterexample": item}));
                }
            }
            tool_results.push(result);
        }
    }
    let ok = blocking_findings.is_empty() && counterexamples.is_empty();
    Ok(json!({
        "kind": "VeritasMathValidationReport",
        "ok": ok,
        "status": if ok { "passed" } else { "failed" },
        "formula_count": formulas.len(),
        "tool_results": tool_results,
        "blocking_findings": blocking_findings,
        "counterexamples": counterexamples,
        "service_url": state.math_tools_url.clone(),
        "required_before_codegen": true,
        "tool_sequence": DEFAULT_TOOL_SEQUENCE,
    }))
}

async fn validate_formula_list_without_workspace(state: &AppState, formulas: &[FormulaCandidate]) -> Result<Value, ApiFailure> {
    let mut all = Vec::new();
    let mut blocking = Vec::new();
    let mut counterexamples = Vec::new();
    for formula in formulas {
        for result in validate_single_formula(state, formula).await {
            if !result.get("ok").and_then(Value::as_bool).unwrap_or(false)
                && result.get("blocks_codegen").and_then(Value::as_bool).unwrap_or(true)
            {
                blocking.push(json!({"formula_id": formula.id, "tool_name": result.get("tool_name"), "result": result}));
            }
            if let Some(items) = result.pointer("/result/counterexamples").and_then(Value::as_array) {
                for item in items {
                    counterexamples.push(json!({"formula_id": formula.id, "tool_name": result.get("tool_name"), "counterexample": item}));
                }
            }
            all.push(result);
        }
    }
    let ok = blocking.is_empty() && counterexamples.is_empty();
    Ok(json!({
        "kind": "VeritasMathValidationReport",
        "ok": ok,
        "status": if ok { "passed" } else { "failed" },
        "formula_count": formulas.len(),
        "tool_results": all,
        "blocking_findings": blocking,
        "counterexamples": counterexamples,
        "service_url": state.math_tools_url.clone(),
        "required_before_codegen": true,
        "tool_sequence": DEFAULT_TOOL_SEQUENCE,
    }))
}

async fn validate_single_formula(state: &AppState, formula: &FormulaCandidate) -> Vec<Value> {
    let mut results = Vec::new();
    let latex = if formula.normalized_latex.trim().is_empty() { formula.latex.clone() } else { formula.normalized_latex.clone() };
    let base_payload = json!({
        "latex": latex,
        "assumptions": formula.assumptions,
        "variables": formula.variables,
        "metadata": formula.metadata,
    });
    for tool in tool_sequence() {
        let payload = tool_payload(&tool, &base_payload);
        match call_tool(state, &tool, &payload).await {
            Ok(result) => results.push(result),
            Err(error) => results.push(json!({
                "ok": false,
                "tool_name": tool,
                "tool_call_id": format!("{}-transport", tool),
                "input_hash": "unavailable_transport_failure",
                "output_hash": "unavailable_transport_failure",
                "status": "failed",
                "blocks_codegen": true,
                "result": {"code": error.code, "message": error.message, "remediation": error.remediation, "details": error.details},
                "duration_ms": 0,
                "service": "veritas-api-math-tool-client",
            })),
        }
    }
    results
}

fn tool_sequence() -> Vec<String> {
    std::env::var("VERITAS_MATH_TOOL_SEQUENCE")
        .ok()
        .map(|value| value.split(',').map(|item| item.trim().to_string()).filter(|item| !item.is_empty()).collect::<Vec<_>>())
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| DEFAULT_TOOL_SEQUENCE.iter().map(|item| (*item).to_string()).collect())
}

fn tool_payload(tool: &str, base: &Value) -> Value {
    let mut payload = base.clone();
    if tool == "generate_property_tests" {
        payload["target_language"] = json!(std::env::var("VERITAS_MATH_PROPERTY_TEST_LANGUAGE").unwrap_or_else(|_| "python".to_string()));
        payload["function_name"] = json!(std::env::var("VERITAS_MATH_PROPERTY_TEST_FUNCTION").unwrap_or_else(|_| "generated_function".to_string()));
    }
    if tool == "numeric_validate" || tool == "counterexample_search" {
        payload["samples"] = json!(u64_env("VERITAS_MATH_TOOLS_DEFAULT_SAMPLES", 21));
        payload["tolerance"] = json!(float_env("VERITAS_MATH_TOOLS_TOLERANCE", 1e-8));
    }
    payload
}

async fn call_tool(state: &AppState, tool: &str, payload: &Value) -> Result<Value, ApiFailure> {
    let route = tool_route(tool)?;
    let url = format!("{}/{}", state.math_tools_url.trim_end_matches('/'), route.trim_start_matches('/'));
    let response = timeout(Duration::from_secs(state.math_tools_timeout_secs), state.http.post(&url).json(payload).send()).await
        .map_err(|_| ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.timeout", format!("Math tool `{tool}` timed out after {}s", state.math_tools_timeout_secs), "Increase VERITAS_MATH_TOOLS_TIMEOUT_SECS or simplify the formula."))?
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.transport", format!("Math tool `{tool}` transport failed: {error}"), "Start the math-tools service and verify VERITAS_MATH_TOOLS_URL."))?;
    let status = response.status();
    let body = response.json::<Value>().await.unwrap_or_else(|_| json!({"ok": false, "status": "failed", "message": "Tool response was not JSON."}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "math_tools.upstream", format!("Math tool `{tool}` returned HTTP {}", status.as_u16()), "Inspect math-tools logs and tool input.").with_details(body));
    }
    Ok(body)
}

fn tool_route(tool: &str) -> Result<String, ApiFailure> {
    let cleaned = tool.trim().to_ascii_lowercase();
    let allowed = tool_sequence();
    if !allowed.iter().any(|item| item == &cleaned) {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "math_tools.unknown_tool", format!("Unknown or disabled math tool `{tool}`."), "Set VERITAS_MATH_TOOL_SEQUENCE to include this tool or use /math-tools/status to inspect configured tools."));
    }
    if !cleaned.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "math_tools.invalid_tool_name", format!("Invalid math tool name `{tool}`."), "Tool names may contain only lowercase ASCII letters, digits, and underscores."));
    }
    Ok(format!("/tools/{cleaned}"))
}

async fn collect_formula_candidates(workspace: &Path) -> Result<Vec<FormulaCandidate>, ApiFailure> {
    let mut by_id: BTreeMap<String, FormulaCandidate> = BTreeMap::new();
    if let Some(registry) = read_json_file(&workspace.join("evidence_registry.json")).await {
        for formula in registry.get("eligible_formulas").and_then(Value::as_array).cloned().unwrap_or_default() {
            if let Some(candidate) = candidate_from_value(&formula) {
                by_id.insert(candidate.id.clone(), candidate);
            }
        }
    }
    if by_id.is_empty() {
        if let Some(manifest) = read_json_file(&workspace.join("formula_manifest.json")).await {
            for formula in manifest.get("records").and_then(Value::as_array).cloned().unwrap_or_default() {
                if let Some(candidate) = candidate_from_value(&formula) {
                    by_id.insert(candidate.id.clone(), candidate);
                }
            }
        }
    }
    if by_id.is_empty() && workspace.join("formulas.jsonl").exists() {
        let rows = crate::read_events_tail(&workspace.join("formulas.jsonl"), 5000).await.unwrap_or_default();
        for formula in rows {
            if let Some(candidate) = candidate_from_value(&formula) {
                by_id.insert(candidate.id.clone(), candidate);
            }
        }
    }
    Ok(by_id.into_values().collect())
}

fn candidate_from_value(value: &Value) -> Option<FormulaCandidate> {
    let latex = value.get("normalized_latex").or_else(|| value.get("latex")).or_else(|| value.get("raw_latex")).and_then(Value::as_str)?.trim().to_string();
    if latex.is_empty() { return None; }
    let id = value.get("formula_id").or_else(|| value.get("id")).and_then(Value::as_str).unwrap_or("formula").to_string();
    Some(FormulaCandidate {
        id,
        normalized_latex: value.get("normalized_latex").and_then(Value::as_str).unwrap_or(&latex).to_string(),
        latex,
        assumptions: string_array(value.get("assumptions")),
        variables: string_array(value.get("variables")),
        metadata: value.clone(),
    })
}

pub(crate) async fn math_heavy(workspace: &Path, goal: &str, plan: &Value) -> bool {
    if workspace.join("formula_manifest.json").exists() || workspace.join("formulas.jsonl").exists() || workspace.join("evidence_registry.json").exists() {
        return true;
    }
    if plan.get("symbolic_shadows").is_some() || plan.get("math_readiness").is_some() || plan.get("representation_map").is_some() {
        return true;
    }
    if plan.get("steps").and_then(Value::as_array).map(|steps| steps.iter().any(|step| step.get("tool").and_then(Value::as_str) == Some("math_reasoning"))).unwrap_or(false) {
        return true;
    }
    let lower = goal.to_ascii_lowercase();
    let indicators = std::env::var("VERITAS_MATH_HEAVY_KEYWORDS").unwrap_or_else(|_| "formula,theorem,latex,invariant,symbolic,equation,proof,gradient,matrix,tensor,derivative,integral,optimization,loss function".to_string());
    indicators.split(',').map(|item| item.trim().to_ascii_lowercase()).filter(|item| !item.is_empty()).any(|needle| lower.contains(&needle))
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}

fn float_env(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|value| value.parse().ok()).unwrap_or(default)
}
