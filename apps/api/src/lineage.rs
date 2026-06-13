use axum::http::StatusCode;
use serde_json::{json, Value};
use std::{collections::BTreeSet, env, path::Path};

use crate::{read_events_tail, read_json_file, ApiFailure};

const LINEAGE_ARTIFACTS: &[&str] = &[
    "evidence_manifest.json",
    "formula_manifest.json",
    "citation_manifest.json",
    "evidence_registry.json",
    "evidence_eligibility.json",
    "review_queue.json",
    "planning_context.json",
];

const EVIDENCE_ID_KEYS: &[&str] = &[
    "evidence_id",
    "source_document_id",
    "document_id",
    "doc_id",
    "paper_id",
    "chunk_id",
    "retrieval_result_id",
    "id",
    "_id",
];

const CITATION_ID_KEYS: &[&str] = &[
    "citation_id",
    "source_citation_id",
    "doi",
    "url",
];

const FORMULA_ID_KEYS: &[&str] = &[
    "formula_id",
    "formula_record_id",
    "symbolic_shadow_id",
];

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value.get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn configured_id_keys(env_key: &str, defaults: &[&str]) -> Vec<String> {
    let configured = env::var(env_key).unwrap_or_default();
    let values: Vec<String> = configured
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    if values.is_empty() {
        defaults.iter().map(|value| value.to_string()).collect()
    } else {
        values
    }
}

fn insert_value_ids_dynamic(value: &Value, keys: &[String], out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if keys.iter().any(|candidate| candidate == key) {
                    if let Some(text) = child.as_str() {
                        let text = text.trim();
                        if !text.is_empty() { out.insert(text.to_string()); }
                    } else if let Some(number) = child.as_i64() {
                        out.insert(number.to_string());
                    }
                }
                insert_value_ids_dynamic(child, keys, out);
            }
        }
        Value::Array(items) => {
            for item in items { insert_value_ids_dynamic(item, keys, out); }
        }
        _ => {}
    }
}

fn collect_ids_from_values_dynamic(values: &[&Value], keys: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for value in values { insert_value_ids_dynamic(value, keys, &mut out); }
    out
}

fn plan_step_ids(plan: &Value) -> BTreeSet<String> {
    plan.get("steps").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter()
        .filter_map(|step| step.get("id").and_then(Value::as_str))
        .map(str::trim).filter(|id| !id.is_empty()).map(ToString::to_string).collect()
}

fn plan_risk_ids(plan: &Value) -> BTreeSet<String> {
    plan.get("risks").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter()
        .filter_map(|risk| risk.get("id").or_else(|| risk.get("risk_id")).and_then(Value::as_str))
        .map(str::trim).filter(|id| !id.is_empty()).map(ToString::to_string).collect()
}

fn plan_validation_gate_ids(plan: &Value) -> BTreeSet<String> {
    plan.get("validation_gates").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter()
        .filter_map(|gate| gate.get("id").or_else(|| gate.get("validation_gate_id")).and_then(Value::as_str))
        .map(str::trim).filter(|id| !id.is_empty()).map(ToString::to_string).collect()
}

fn plan_human_checkpoint_ids(plan: &Value) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = crate::gates::human::required_pre_codegen_checkpoint_phases().into_iter().collect();
    if let Some(steps) = plan.get("steps").and_then(Value::as_array) {
        for step in steps {
            for id in string_array(step, "human_checkpoint_ids") { ids.insert(id); }
            if let Some(id) = step.get("human_checkpoint_id").and_then(Value::as_str) { ids.insert(id.to_string()); }
        }
    }
    ids
}

pub(crate) async fn load_workspace_lineage_context(workspace: &Path, plan_envelope: &Value, plan: &Value) -> Value {
    let mut artifact_values = Vec::new();
    for file in LINEAGE_ARTIFACTS {
        if let Some(value) = read_json_file(&workspace.join(file)).await {
            artifact_values.push(value);
        }
    }
    let mut refs: Vec<&Value> = artifact_values.iter().collect();
    if let Some(evidence) = plan_envelope.get("evidence") { refs.push(evidence); }

    let evidence_keys = configured_id_keys("VERITAS_LINEAGE_EVIDENCE_ID_FIELDS", EVIDENCE_ID_KEYS);
    let citation_keys = configured_id_keys("VERITAS_LINEAGE_CITATION_ID_FIELDS", CITATION_ID_KEYS);
    let formula_keys = configured_id_keys("VERITAS_LINEAGE_FORMULA_ID_FIELDS", FORMULA_ID_KEYS);
    let evidence_ids = collect_ids_from_values_dynamic(&refs, &evidence_keys);
    let citation_ids = collect_ids_from_values_dynamic(&refs, &citation_keys);
    let formula_ids = collect_ids_from_values_dynamic(&refs, &formula_keys);
    let validation_gate_ids = plan_validation_gate_ids(plan);
    let human_checkpoint_ids = plan_human_checkpoint_ids(plan);
    let risk_ids = plan_risk_ids(plan);
    let step_ids = plan_step_ids(plan);

    json!({
        "kind": "VeritasLineageContext",
        "artifact_sources": LINEAGE_ARTIFACTS,
        "lineage_id_field_config": {
            "evidence": evidence_keys,
            "citation": citation_keys,
            "formula": formula_keys
        },
        "plan_step_ids": step_ids.into_iter().collect::<Vec<_>>(),
        "evidence_ids": evidence_ids.into_iter().collect::<Vec<_>>(),
        "citation_ids": citation_ids.into_iter().collect::<Vec<_>>(),
        "formula_ids": formula_ids.into_iter().collect::<Vec<_>>(),
        "risk_ids": risk_ids.into_iter().collect::<Vec<_>>(),
        "validation_gate_ids": validation_gate_ids.into_iter().collect::<Vec<_>>(),
        "human_checkpoint_ids": human_checkpoint_ids.into_iter().collect::<Vec<_>>()
    })
}

pub(crate) fn id_summary_from_values(values: &[&Value]) -> Value {
    let evidence_keys = configured_id_keys("VERITAS_LINEAGE_EVIDENCE_ID_FIELDS", EVIDENCE_ID_KEYS);
    let citation_keys = configured_id_keys("VERITAS_LINEAGE_CITATION_ID_FIELDS", CITATION_ID_KEYS);
    let formula_keys = configured_id_keys("VERITAS_LINEAGE_FORMULA_ID_FIELDS", FORMULA_ID_KEYS);
    json!({
        "evidence_ids": collect_ids_from_values_dynamic(values, &evidence_keys).into_iter().collect::<Vec<_>>(),
        "citation_ids": collect_ids_from_values_dynamic(values, &citation_keys).into_iter().collect::<Vec<_>>(),
        "formula_ids": collect_ids_from_values_dynamic(values, &formula_keys).into_iter().collect::<Vec<_>>(),
        "required_human_checkpoint_ids": crate::gates::human::required_pre_codegen_checkpoint_phases(),
    })
}

fn array_field_subset(item: &Value, field: &str, allowed: &BTreeSet<String>, errors: &mut Vec<String>, subject: &str, require_non_empty: bool) {
    let values = string_array(item, field);
    if require_non_empty && values.is_empty() {
        errors.push(format!("{subject}.{field} must contain at least one lineage id"));
        return;
    }
    if allowed.is_empty() && !values.is_empty() {
        errors.push(format!("{subject}.{field} references lineage ids but no allowed ids were available in workspace artifacts"));
        return;
    }
    for id in values {
        if !allowed.contains(&id) {
            errors.push(format!("{subject}.{field} references unknown id `{id}`"));
        }
    }
}

fn set_from_json_array(context: &Value, key: &str) -> BTreeSet<String> {
    context.get(key).and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter()
        .filter_map(Value::as_str).map(ToString::to_string).collect()
}

pub(crate) fn validate_plan_lineage_references(plan: &Value, context: &Value) -> Result<(), ApiFailure> {
    let allowed_evidence = set_from_json_array(context, "evidence_ids");
    let allowed_citations = set_from_json_array(context, "citation_ids");
    let allowed_formulas = set_from_json_array(context, "formula_ids");
    let allowed_risks = set_from_json_array(context, "risk_ids");
    let allowed_validation = set_from_json_array(context, "validation_gate_ids");
    let allowed_human = set_from_json_array(context, "human_checkpoint_ids");

    let mut errors = Vec::new();
    for (idx, step) in plan.get("steps").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter().enumerate() {
        let subject = format!("plan.steps[{idx}]");
        array_field_subset(step, "evidence_ids", &allowed_evidence, &mut errors, &subject, true);
        array_field_subset(step, "citation_ids", &allowed_citations, &mut errors, &subject, true);
        array_field_subset(step, "formula_ids", &allowed_formulas, &mut errors, &subject, true);
        array_field_subset(step, "risk_ids", &allowed_risks, &mut errors, &subject, true);
        array_field_subset(step, "validation_gate_ids", &allowed_validation, &mut errors, &subject, true);
        array_field_subset(step, "human_checkpoint_ids", &allowed_human, &mut errors, &subject, true);
    }

    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(
        StatusCode::FAILED_DEPENDENCY,
        "lineage.plan_invalid",
        "Planner output referenced lineage ids that are missing from the approved evidence, citation, formula, risk, validation, or human-checkpoint context.",
        "Review evidence and citations/formulas, then regenerate the plan so every step cites only ids present in planning_context/evidence_registry artifacts."
    ).with_details(json!({"errors": errors, "lineage_context": context, "plan": plan}))) }
}

pub(crate) fn validate_codegen_lineage(generated: &Value, plan: &Value, context: &Value) -> Result<(), ApiFailure> {
    let allowed_steps = set_from_json_array(context, "plan_step_ids");
    let allowed_evidence = set_from_json_array(context, "evidence_ids");
    let allowed_citations = set_from_json_array(context, "citation_ids");
    let allowed_formulas = set_from_json_array(context, "formula_ids");
    let allowed_validation = set_from_json_array(context, "validation_gate_ids");

    let mut errors = Vec::new();
    if allowed_steps.is_empty() { errors.push("No plan step ids exist; generated files cannot be traced to plan steps.".to_string()); }
    if allowed_evidence.is_empty() { errors.push("No evidence ids exist; generated files cannot be traced to evidence.".to_string()); }
    if allowed_citations.is_empty() { errors.push("No citation ids exist; generated files cannot be traced to citation provenance.".to_string()); }
    if allowed_formulas.is_empty() { errors.push("No formula ids exist; generated files cannot be traced to approved formula records.".to_string()); }
    if allowed_validation.is_empty() { errors.push("No validation gate ids exist; generated files cannot be traced to validation obligations.".to_string()); }

    for (idx, file) in generated.get("files").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter().enumerate() {
        let subject = format!("codegen.files[{idx}]");
        array_field_subset(file, "derived_from_plan_step_ids", &allowed_steps, &mut errors, &subject, true);
        array_field_subset(file, "derived_from_evidence_ids", &allowed_evidence, &mut errors, &subject, true);
        array_field_subset(file, "derived_from_citation_ids", &allowed_citations, &mut errors, &subject, true);
        array_field_subset(file, "derived_from_formula_ids", &allowed_formulas, &mut errors, &subject, true);
        array_field_subset(file, "required_validation_ids", &allowed_validation, &mut errors, &subject, true);
    }
    for (idx, command) in generated.get("commands").and_then(Value::as_array).map(|items| items.as_slice()).unwrap_or(&[]).iter().enumerate() {
        let subject = format!("codegen.commands[{idx}]");
        array_field_subset(command, "derived_from_plan_step_ids", &allowed_steps, &mut errors, &subject, true);
        array_field_subset(command, "required_validation_ids", &allowed_validation, &mut errors, &subject, true);
    }

    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(
        StatusCode::BAD_GATEWAY,
        "lineage.codegen_invalid",
        "Code model output failed Veritas lineage enforcement before file writes.",
        "Regenerate code with file and command lineage that references only approved plan, evidence, citation, formula, and validation ids. No generated file was written."
    ).with_details(json!({"errors": errors, "lineage_context": context, "generated": generated, "plan": plan}))) }
}

pub(crate) fn codegen_lineage_contract(plan: &Value, context: &Value) -> Value {
    json!({
        "rule": "Every generated file and validation command must cite only these ids. The application rejects unknown or empty lineage arrays before writing files.",
        "allowed_plan_step_ids": context.get("plan_step_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_evidence_ids": context.get("evidence_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_citation_ids": context.get("citation_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_formula_ids": context.get("formula_ids").cloned().unwrap_or_else(|| json!([])),
        "allowed_validation_gate_ids": context.get("validation_gate_ids").cloned().unwrap_or_else(|| json!([])),
        "planner_steps": plan.get("steps").cloned().unwrap_or_else(|| json!([])),
        "file_required_fields": [
            "path", "content", "purpose", "derived_from_plan_step_ids", "derived_from_evidence_ids", "derived_from_citation_ids", "derived_from_formula_ids", "required_validation_ids"
        ],
        "command_required_fields": ["command", "purpose", "derived_from_plan_step_ids", "required_validation_ids"]
    })
}

pub(crate) async fn build_run_lineage(
    workspace: &Path,
    plan_envelope: &Value,
    plan: &Value,
    code_package: &Value,
    commands_run: &[Value],
    validation_results: &[Value],
    retry_history: &[Value],
    artifact_decision: &Value,
) -> Value {
    let context = load_workspace_lineage_context(workspace, plan_envelope, plan).await;
    let human_checkpoints = read_events_tail(&workspace.join("human_checkpoints.jsonl"), 2000).await.unwrap_or_default();
    let gate_decisions = read_events_tail(&workspace.join("gate_decisions.jsonl"), 2000).await.unwrap_or_default();
    let source_documents = read_json_file(&workspace.join("source_manifest.json")).await
        .or_else(|| read_json_file(&workspace.join("evidence_manifest.json")).await)
        .unwrap_or_else(|| json!({"status":"missing", "path":"source_manifest.json"}));
    let citations = read_json_file(&workspace.join("citation_manifest.json")).await.unwrap_or_else(|| json!({"status":"missing", "path":"citation_manifest.json"}));
    let formulas = read_json_file(&workspace.join("formula_manifest.json")).await.unwrap_or_else(|| json!({"status":"missing", "path":"formula_manifest.json"}));
    let review_decisions = json!({
        "evidence_registry": read_json_file(&workspace.join("evidence_registry.json")).await,
        "evidence_eligibility": read_json_file(&workspace.join("evidence_eligibility.json")).await,
        "human_checkpoints": human_checkpoints,
    });
    let representation_model = read_json_file(&workspace.join("representation_model.json")).await.unwrap_or_else(|| json!({"status":"not_required_or_missing"}));
    let planning_context = read_json_file(&workspace.join("planning_context.json")).await.unwrap_or_else(|| context.clone());
    let files = code_package.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    let file_lineage: Vec<Value> = files.into_iter().map(|file| json!({
        "path": file.get("path"),
        "purpose": file.get("purpose"),
        "derived_from_plan_step_ids": file.get("derived_from_plan_step_ids"),
        "derived_from_evidence_ids": file.get("derived_from_evidence_ids"),
        "derived_from_citation_ids": file.get("derived_from_citation_ids"),
        "derived_from_formula_ids": file.get("derived_from_formula_ids"),
        "required_validation_ids": file.get("required_validation_ids"),
    })).collect();
    let command_lineage: Vec<Value> = commands_run.iter().enumerate().map(|(idx, command)| json!({
        "index": idx,
        "command": command.get("command"),
        "success": command.get("success"),
        "exit_code": command.get("exit_code"),
    })).collect();
    json!({
        "source_documents": source_documents,
        "citations": citations,
        "formulas": formulas,
        "review_decisions": review_decisions,
        "representation_model": representation_model,
        "planning_context": planning_context,
        "plan_lineage": {"context": context, "plan": plan},
        "file_lineage": file_lineage,
        "command_lineage": command_lineage,
        "validation_lineage": validation_results,
        "repair_lineage": retry_history,
        "governance_lineage": {"gate_decisions": gate_decisions, "artifact_decision": artifact_decision},
    })
}

pub(crate) async fn build_lineage_context(workspace: &Path, plan_envelope: &Value, plan: &Value) -> Result<Value, ApiFailure> {
    Ok(load_workspace_lineage_context(workspace, plan_envelope, plan).await)
}

pub(crate) async fn write_planning_context(workspace: &Path, context: &Value) -> Result<(), ApiFailure> {
    let existing = read_json_file(&workspace.join("planning_context.json")).await.unwrap_or_else(|| json!({}));
    let mut planning_context = if existing.is_object() { existing } else { json!({}) };
    planning_context["kind"] = planning_context.get("kind").cloned().unwrap_or_else(|| json!("VeritasPlanningContext"));
    planning_context["lineage_context"] = context.clone();
    planning_context["approved_evidence_ids"] = planning_context.get("approved_evidence_ids").cloned().unwrap_or_else(|| context.get("evidence_ids").cloned().unwrap_or_else(|| json!([])));
    planning_context["approved_citation_ids"] = planning_context.get("approved_citation_ids").cloned().unwrap_or_else(|| context.get("citation_ids").cloned().unwrap_or_else(|| json!([])));
    planning_context["eligible_formula_ids"] = planning_context.get("eligible_formula_ids").cloned().unwrap_or_else(|| context.get("formula_ids").cloned().unwrap_or_else(|| json!([])));
    planning_context["validation_gate_ids"] = context.get("validation_gate_ids").cloned().unwrap_or_else(|| json!([]));
    planning_context["human_checkpoint_ids"] = context.get("human_checkpoint_ids").cloned().unwrap_or_else(|| json!([]));
    planning_context["lineage_status"] = json!("ready_for_lineage_validated_planning");
    crate::write_json_file(&workspace.join("planning_context.json"), &planning_context).await
}

pub(crate) fn validate_plan_lineage(plan: &Value, context: &Value) -> Result<(), ApiFailure> {
    validate_plan_lineage_references(plan, context)
}

pub(crate) fn validate_codegen_lineage_for_plan(plan: &Value, context: &Value, generated: &Value) -> Result<(), ApiFailure> {
    validate_codegen_lineage(generated, plan, context)
}

pub(crate) async fn write_file_lineage(workspace: &Path, generated: &Value) -> Result<Value, ApiFailure> {
    let files = generated.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    let file_lineage: Vec<Value> = files.into_iter().map(|file| json!({
        "path": file.get("path"),
        "purpose": file.get("purpose"),
        "derived_from_plan_step_ids": file.get("derived_from_plan_step_ids"),
        "derived_from_evidence_ids": file.get("derived_from_evidence_ids"),
        "derived_from_citation_ids": file.get("derived_from_citation_ids"),
        "derived_from_formula_ids": file.get("derived_from_formula_ids"),
        "required_validation_ids": file.get("required_validation_ids"),
    })).collect();
    let value = json!({"kind":"VeritasFileLineage", "files": file_lineage});
    crate::write_json_file(&workspace.join("file_lineage.json"), &value).await?;
    Ok(value)
}

pub(crate) async fn build_report_lineage(
    workspace: &Path,
    plan_envelope: &Value,
    plan: &Value,
    code_package: &Value,
    commands_run: &[Value],
    validation_results: &[Value],
    retry_history: &[Value],
    artifact_decision: &Value,
) -> Result<Value, ApiFailure> {
    Ok(build_run_lineage(workspace, plan_envelope, plan, code_package, commands_run, validation_results, retry_history, artifact_decision).await)
}
