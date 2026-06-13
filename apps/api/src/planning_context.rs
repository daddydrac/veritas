use axum::http::StatusCode;
use serde_json::{json, Value};
use std::{collections::BTreeSet, env, path::{Path, PathBuf}};

use crate::{read_json_file, write_json_file, ApiFailure};

const CONTEXT_KIND: &str = "VeritasPlanningContext";
const DEFAULT_EXECUTION_MODE: &str = "production";
const DEV_EXPLORATORY_MODE: &str = "dev_exploratory";

pub(crate) struct PlanningContextInput<'a> {
    pub(crate) workspace: Option<&'a Path>,
    pub(crate) goal: &'a str,
    pub(crate) size: u32,
    pub(crate) opensearch_evidence: &'a Value,
    pub(crate) opensearch_error: Option<Value>,
    pub(crate) formula_trace: &'a Value,
    pub(crate) ontology_facts: &'a Value,
    pub(crate) request_context: Option<&'a Value>,
}

fn bool_env(name: &str, default: bool) -> bool {
    env::var(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_list(name: &str, defaults: &[&str]) -> Vec<String> {
    let values: Vec<String> = env::var(name).unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    if values.is_empty() { defaults.iter().map(|value| value.to_string()).collect() } else { values }
}

fn execution_mode(input: &PlanningContextInput<'_>) -> String {
    input.request_context
        .and_then(|value| value.get("execution_mode"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| env::var("VERITAS_PLANNING_EXECUTION_MODE").ok())
        .unwrap_or_else(|| DEFAULT_EXECUTION_MODE.to_string())
}

fn production_bound(mode: &str) -> bool {
    !mode.eq_ignore_ascii_case(DEV_EXPLORATORY_MODE)
}

fn string_value(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).map(ToString::to_string)
}

fn bool_value(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn status_text(value: &Value) -> String {
    for key in env_list("VERITAS_PLANNING_STATUS_FIELDS", &[
        "review_decision",
        "normalized_review_status",
        "citation_review_status",
        "codegen_eligibility_status",
        "human_validation_status",
        "status",
    ]) {
        if let Some(text) = string_value(value, &key) { return text.to_ascii_lowercase(); }
    }
    String::new()
}

fn rejected_status(value: &Value) -> bool {
    let status = status_text(value);
    env_list("VERITAS_PLANNING_REJECTED_STATUSES", &["reject", "rejected", "blocked", "not_eligible", "not_usable"])
        .iter()
        .any(|needle| status.contains(&needle.to_ascii_lowercase()))
}

fn approved_status(value: &Value) -> bool {
    let status = status_text(value);
    env_list("VERITAS_PLANNING_APPROVED_STATUSES", &["approve", "approved", "eligible", "eligible_for_evidence_backed_planning", "eligible_human_validated"])
        .iter()
        .any(|needle| status.contains(&needle.to_ascii_lowercase()))
}

fn collect_id_fields(value: &Value, keys: &[String], out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if keys.iter().any(|candidate| candidate == key) {
                    if let Some(text) = child.as_str().map(str::trim).filter(|text| !text.is_empty()) {
                        out.insert(text.to_string());
                    } else if let Some(number) = child.as_i64() {
                        out.insert(number.to_string());
                    }
                }
                collect_id_fields(child, keys, out);
            }
        }
        Value::Array(items) => {
            for item in items { collect_id_fields(item, keys, out); }
        }
        _ => {}
    }
}

fn ids_from_value(value: &Value, env_name: &str, defaults: &[&str]) -> Vec<String> {
    let keys = env_list(env_name, defaults);
    let mut out = BTreeSet::new();
    collect_id_fields(value, &keys, &mut out);
    out.into_iter().collect()
}

fn array_items<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value.get(key).and_then(Value::as_array).map(|items| items.iter().collect()).unwrap_or_default()
}

async fn load_registry(input: &PlanningContextInput<'_>) -> (Option<PathBuf>, Option<Value>) {
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(path) = input.request_context
        .and_then(|value| value.get("evidence_registry_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from) {
        candidates.push(path);
    }
    if let Some(path) = input.request_context
        .and_then(|value| value.get("evidence_manifest_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from) {
        if let Some(manifest) = read_json_file(&path).await {
            if let Some(registry_path) = manifest.get("evidence_registry_path").and_then(Value::as_str).filter(|text| !text.trim().is_empty()) {
                candidates.push(PathBuf::from(registry_path));
            }
        }
        if let Some(parent) = path.parent() { candidates.push(parent.join("evidence_registry.json")); }
    }
    if let Some(workspace) = input.workspace {
        candidates.push(workspace.join("evidence_registry.json"));
        if let Some(manifest) = read_json_file(&workspace.join("evidence_manifest.json")).await {
            if let Some(registry_path) = manifest.get("evidence_registry_path").and_then(Value::as_str).filter(|text| !text.trim().is_empty()) {
                candidates.push(PathBuf::from(registry_path));
            }
        }
    }
    if let Ok(path) = env::var("VERITAS_EVIDENCE_REGISTRY_PATH") {
        if !path.trim().is_empty() { candidates.push(PathBuf::from(path)); }
    }
    for path in candidates {
        if let Some(value) = read_json_file(&path).await { return (Some(path), Some(value)); }
    }
    (None, None)
}

fn record_ids(record: &Value) -> Vec<String> {
    ids_from_value(record, "VERITAS_PLANNING_EVIDENCE_ID_FIELDS", &[
        "evidence_id", "retrieval_result_id", "chunk_id", "source_document_id", "paper_id", "doc_id", "id", "_id",
    ])
}

fn citation_id(record: &Value) -> Option<String> {
    for key in env_list("VERITAS_PLANNING_CITATION_ID_FIELDS", &["citation_id", "source_citation_id", "doi", "url", "paper_id", "source_document_id"]) {
        if let Some(value) = string_value(record, &key) { return Some(value); }
    }
    None
}

fn formula_id(record: &Value) -> Option<String> {
    for key in env_list("VERITAS_PLANNING_FORMULA_ID_FIELDS", &["formula_id", "formula_record_id", "symbolic_shadow_id", "id"]) {
        if let Some(value) = string_value(record, &key) { return Some(value); }
    }
    None
}

fn citation_approved(record: &Value) -> bool {
    if rejected_status(record) { return false; }
    bool_value(record, "citation_usable_for_audit")
        || bool_value(record, "eligible_for_planning")
        || bool_value(record, "citation_human_validated")
        || approved_status(record)
}

fn formula_eligible(record: &Value) -> bool {
    if rejected_status(record) { return false; }
    bool_value(record, "eligible_for_codegen")
        || bool_value(record, "use_for_codegen")
        || string_value(record, "codegen_eligibility_status").map(|status| status == "eligible").unwrap_or(false)
}

fn gather_registry_ids(registry: &Value) -> (Vec<Value>, Vec<Value>, Vec<String>, Vec<String>, Vec<String>) {
    let mut approved_citations = Vec::new();
    let mut eligible_formulas = Vec::new();
    let mut evidence_ids = BTreeSet::new();
    let mut citation_ids = BTreeSet::new();
    let mut formula_ids = BTreeSet::new();
    for citation in array_items(registry, "citations") {
        if citation_approved(citation) {
            for id in record_ids(citation) { evidence_ids.insert(id); }
            if let Some(id) = citation_id(citation) { citation_ids.insert(id); }
            approved_citations.push(citation.clone());
        }
    }
    for formula in array_items(registry, "formulas") {
        if formula_eligible(formula) {
            for id in record_ids(formula) { evidence_ids.insert(id); }
            if let Some(id) = formula_id(formula) { formula_ids.insert(id); }
            if let Some(id) = citation_id(formula) { citation_ids.insert(id); }
            eligible_formulas.push(formula.clone());
        }
    }
    if let Some(planning) = registry.get("planning") {
        for key in env_list("VERITAS_PLANNING_APPROVED_CITATION_ARRAY_FIELDS", &["usable_citation_ids", "approved_citation_ids"]) {
            if let Some(items) = planning.get(&key).and_then(Value::as_array) {
                for id in items.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) { citation_ids.insert(id.to_string()); }
            }
        }
        for key in env_list("VERITAS_PLANNING_ELIGIBLE_FORMULA_ARRAY_FIELDS", &["eligible_formula_ids", "usable_formula_ids"]) {
            if let Some(items) = planning.get(&key).and_then(Value::as_array) {
                for id in items.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) { formula_ids.insert(id.to_string()); }
            }
        }
    }
    (approved_citations, eligible_formulas, evidence_ids.into_iter().collect(), citation_ids.into_iter().collect(), formula_ids.into_iter().collect())
}

fn gather_retrieved_approved_ids(evidence: &Value) -> (Vec<Value>, Vec<String>, Vec<String>, Vec<String>) {
    let mut approved_items = Vec::new();
    let mut evidence_ids = BTreeSet::new();
    let mut citation_ids = BTreeSet::new();
    let mut formula_ids = BTreeSet::new();
    for hit in evidence.get("hits").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]) {
        let source = hit.get("source").unwrap_or(hit);
        let approved = citation_approved(source)
            || bool_value(source, "human_validated")
            || bool_value(source, "approved_for_planning")
            || bool_value(source, "eligible_for_planning");
        if approved {
            for id in record_ids(hit) { evidence_ids.insert(id); }
            for id in record_ids(source) { evidence_ids.insert(id); }
            if let Some(id) = citation_id(source) { citation_ids.insert(id); }
            for formula in array_items(source, "formulas") {
                if let Some(id) = formula_id(formula) { formula_ids.insert(id); }
            }
            approved_items.push(hit.clone());
        }
    }
    (approved_items, evidence_ids.into_iter().collect(), citation_ids.into_iter().collect(), formula_ids.into_iter().collect())
}

fn workspace_artifact_status(workspace: Option<&Path>, filename: &str) -> Value {
    if let Some(path) = workspace {
        let artifact = path.join(filename);
        json!({"path": artifact.display().to_string(), "exists": artifact.exists()})
    } else {
        json!({"path": Value::Null, "exists": false})
    }
}

pub(crate) async fn build(input: PlanningContextInput<'_>) -> Result<Value, ApiFailure> {
    let mode = execution_mode(&input);
    let production = production_bound(&mode);
    let (registry_path, registry_value) = load_registry(&input).await;
    let (approved_citations, eligible_formulas, mut approved_evidence_ids, mut approved_citation_ids, mut eligible_formula_ids) = registry_value
        .as_ref()
        .map(gather_registry_ids)
        .unwrap_or_else(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    let (approved_retrieved_evidence, retrieved_evidence_ids, retrieved_citation_ids, retrieved_formula_ids) = gather_retrieved_approved_ids(input.opensearch_evidence);
    approved_evidence_ids.extend(retrieved_evidence_ids);
    approved_citation_ids.extend(retrieved_citation_ids);
    eligible_formula_ids.extend(retrieved_formula_ids);
    approved_evidence_ids.sort(); approved_evidence_ids.dedup();
    approved_citation_ids.sort(); approved_citation_ids.dedup();
    eligible_formula_ids.sort(); eligible_formula_ids.dedup();
    let planning = registry_value.as_ref().and_then(|value| value.get("planning")).cloned().unwrap_or_else(|| json!({}));
    let registry_planning_allowed = planning.get("allowed").and_then(Value::as_bool).unwrap_or(false);
    let empty_bypass_requested = bool_env("VERITAS_ALLOW_EMPTY_EVIDENCE", false);
    let dev_bypass_allowed = empty_bypass_requested && mode.eq_ignore_ascii_case(DEV_EXPLORATORY_MODE);
    let has_evidence = !approved_evidence_ids.is_empty() || !approved_retrieved_evidence.is_empty();
    let has_citation = !approved_citation_ids.is_empty() || !approved_citations.is_empty();
    let production_ready = (registry_planning_allowed || (has_evidence && has_citation)) && has_evidence;
    let status = if production_ready {
        "ready_for_evidence_backed_planning"
    } else if dev_bypass_allowed {
        "dev_only_unverified"
    } else {
        "blocked_by_evidence"
    };
    let mut blocking_reasons = Vec::<String>::new();
    if registry_value.is_none() && production { blocking_reasons.push("Evidence Eligibility Registry is missing for production-bound planning.".to_string()); }
    if input.opensearch_error.is_some() { blocking_reasons.push("OpenSearch retrieval failed; planning cannot be evidence-grounded from search results.".to_string()); }
    if !has_evidence { blocking_reasons.push("No approved evidence identifiers are available for planning.".to_string()); }
    if !has_citation { blocking_reasons.push("No approved citation identifiers are available for audit-backed planning.".to_string()); }
    if let Some(items) = planning.get("blocking_reasons").and_then(Value::as_array) {
        for item in items.iter().filter_map(Value::as_str).map(str::trim).filter(|text| !text.is_empty()) { blocking_reasons.push(item.to_string()); }
    }
    if dev_bypass_allowed { blocking_reasons.push("VERITAS_ALLOW_EMPTY_EVIDENCE was honored only because execution_mode=dev_exploratory; this cannot produce production-bound artifacts.".to_string()); }
    blocking_reasons.sort(); blocking_reasons.dedup();
    let representation_model = match input.workspace {
        Some(workspace) => read_json_file(&workspace.join("representation_model.json")).await.unwrap_or_else(|| json!({"status":"not_available"})),
        None => json!({"status":"not_available"}),
    };
    let shacl_status = match input.workspace {
        Some(workspace) => read_json_file(&workspace.join("pre_codegen_shacl_report.json")).await
            .or_else(|| read_json_file(&workspace.join("automatic_shacl_report.json")).await)
            .unwrap_or_else(|| json!({"status":"not_run_before_planning"})),
        None => json!({"status":"not_run_before_planning"}),
    };
    let mut lineage_evidence_ids = approved_evidence_ids.clone();
    if lineage_evidence_ids.is_empty() && dev_bypass_allowed {
        for id in ids_from_value(input.opensearch_evidence, "VERITAS_PLANNING_EVIDENCE_ID_FIELDS", &["id", "_id", "chunk_id", "paper_id", "doc_id"]) { lineage_evidence_ids.push(id); }
    }
    lineage_evidence_ids.sort(); lineage_evidence_ids.dedup();
    let context = json!({
        "kind": CONTEXT_KIND,
        "version": env::var("VERITAS_PLANNING_CONTEXT_VERSION").unwrap_or_else(|_| "phase9.0".to_string()),
        "goal": input.goal,
        "requested_size": input.size,
        "execution_mode": mode,
        "production_bound": production && !dev_bypass_allowed,
        "status": status,
        "ok": production_ready || dev_bypass_allowed,
        "registry_path": registry_path.as_ref().map(|path| path.display().to_string()),
        "workspace": input.workspace.map(|path| path.display().to_string()),
        "retrieved_evidence": input.opensearch_evidence,
        "opensearch_error": input.opensearch_error,
        "approved_retrieved_evidence": approved_retrieved_evidence,
        "approved_citations": approved_citations,
        "eligible_formulas": eligible_formulas,
        "approved_evidence_ids": lineage_evidence_ids.clone(),
        "approved_citation_ids": approved_citation_ids.clone(),
        "eligible_formula_ids": eligible_formula_ids.clone(),
        "ontology_facts": input.ontology_facts,
        "formula_trace": input.formula_trace,
        "shacl_status": shacl_status,
        "representation_model": representation_model,
        "source_artifact_status": {
            "evidence_manifest": workspace_artifact_status(input.workspace, "evidence_manifest.json"),
            "formula_manifest": workspace_artifact_status(input.workspace, "formula_manifest.json"),
            "citation_manifest": workspace_artifact_status(input.workspace, "citation_manifest.json"),
            "evidence_registry": workspace_artifact_status(input.workspace, "evidence_registry.json")
        },
        "blocking_reasons": blocking_reasons,
        "next_action": if production_ready || dev_bypass_allowed { "Continue to schema-validated planning." } else { "Review citations/formulas, rebuild evidence_registry.json, and verify retrieval before planning." },
        "allowed_lineage_ids": {
            "evidence_ids": lineage_evidence_ids,
            "citation_ids": approved_citation_ids,
            "formula_ids": eligible_formula_ids
        }
    });
    if production && !production_ready {
        return Err(ApiFailure::new(
            StatusCode::FAILED_DEPENDENCY,
            "planning_context.no_approved_evidence",
            "Production-bound planning requires approved evidence and approved citation provenance.",
            "Run local ingestion, review citations/formulas, rebuild evidence_registry.json, then retry planning. For exploratory work only, set execution_mode=dev_exploratory and VERITAS_ALLOW_EMPTY_EVIDENCE=true."
        ).with_details(context));
    }
    Ok(context)
}

pub(crate) async fn write_context(workspace: Option<&Path>, context: &Value) -> Result<(), ApiFailure> {
    if let Some(workspace) = workspace { write_json_file(&workspace.join("planning_context.json"), context).await?; }
    Ok(())
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value.get(key).and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn allowed_ids(context: &Value, key: &str) -> BTreeSet<String> {
    context.get("allowed_lineage_ids").and_then(|value| value.get(key)).and_then(Value::as_array)
        .or_else(|| context.get(key).and_then(Value::as_array))
        .map(|items| items.iter().filter_map(Value::as_str).map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn validate_step_ids(step: &Value, field: &str, allowed: &BTreeSet<String>, label: &str, errors: &mut Vec<String>) {
    let values = string_array(step, field);
    if values.is_empty() {
        errors.push(format!("{label}.{field} must cite at least one approved planning_context id"));
        return;
    }
    if allowed.is_empty() {
        errors.push(format!("{label}.{field} cannot be validated because planning_context has no allowed ids for that field"));
        return;
    }
    for value in values {
        if !allowed.contains(&value) { errors.push(format!("{label}.{field} references `{value}` which is not approved in planning_context")); }
    }
}

pub(crate) fn validate_plan_references(plan: &Value, context: &Value) -> Result<(), ApiFailure> {
    let evidence_ids = allowed_ids(context, "evidence_ids").into_iter().chain(allowed_ids(context, "approved_evidence_ids")).collect::<BTreeSet<_>>();
    let citation_ids = allowed_ids(context, "citation_ids").into_iter().chain(allowed_ids(context, "approved_citation_ids")).collect::<BTreeSet<_>>();
    let formula_ids = allowed_ids(context, "formula_ids").into_iter().chain(allowed_ids(context, "eligible_formula_ids")).collect::<BTreeSet<_>>();
    let mut errors = Vec::<String>::new();
    for (idx, step) in plan.get("steps").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter().enumerate() {
        let label = format!("plan.steps[{idx}]");
        validate_step_ids(step, "evidence_ids", &evidence_ids, &label, &mut errors);
        validate_step_ids(step, "citation_ids", &citation_ids, &label, &mut errors);
        validate_step_ids(step, "formula_ids", &formula_ids, &label, &mut errors);
    }
    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(
        StatusCode::FAILED_DEPENDENCY,
        "planning_context.plan_not_grounded",
        "Planner output cited evidence, citation, or formula ids that are not approved in planning_context.",
        "Regenerate the plan using the provided planning_context; the planner must cite only approved_evidence_ids, approved_citation_ids, and eligible_formula_ids."
    ).with_details(json!({"errors": errors, "planning_context": context, "plan": plan}))) }
}

pub(crate) fn planner_prompt_contract(context: &Value) -> Value {
    json!({
        "required_behavior": "Planner must cite only approved ids from this contract. Production-bound planning without approved ids is forbidden.",
        "allowed_evidence_ids": context.get("approved_evidence_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_citation_ids": context.get("approved_citation_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_formula_ids": context.get("eligible_formula_ids").cloned().unwrap_or_else(|| json!([])),
        "status": context.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "production_bound": context.get("production_bound").cloned().unwrap_or_else(|| json!(true)),
    })
}
