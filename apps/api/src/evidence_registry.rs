use axum::http::StatusCode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::{ApiFailure, MathToCodeRequest};

const REGISTRY_KIND: &str = "VeritasEvidenceEligibilityRegistry";

#[derive(Clone, Debug)]
pub(crate) struct EvidenceEligibilityDecision {
    pub formula_context: Value,
    pub formula: Value,
    pub citation: Value,
    pub registry_path: Option<PathBuf>,
    pub exploratory_unverified: bool,
}

impl EvidenceEligibilityDecision {
    pub(crate) fn status_floor(&self) -> &'static str {
        if self.exploratory_unverified { "generated_unvalidated" } else { "registry_eligible" }
    }
}

pub(crate) async fn require_math_to_code_eligibility(req: &MathToCodeRequest) -> Result<EvidenceEligibilityDecision, ApiFailure> {
    let requested_formula_id = req.formula_record_id.as_ref().or(req.formula_id.as_ref()).map(|s| s.trim()).filter(|s| !s.is_empty());
    let allow_exploratory = req.allow_exploratory_unverified.unwrap_or(false);

    if requested_formula_id.is_none() {
        if allow_exploratory && req.formula_latex.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
            let dynamic_formula_id = format!("unverified-formula-{}", uuid::Uuid::new_v4().simple());
            let dynamic_citation_id = format!("unverified-citation-{}", uuid::Uuid::new_v4().simple());
            let formula = json!({
                "formula_id": req.formula_id.clone().unwrap_or(dynamic_formula_id),
                "raw_latex": req.formula_latex.clone().unwrap_or_default(),
                "normalized_latex": req.formula_latex.clone().unwrap_or_default(),
                "codegen_eligibility_status": "waived_for_exploration",
                "eligible_for_codegen": false,
                "blocking_reason": "Ad-hoc LaTeX was supplied without an Evidence Eligibility Registry. This can only produce exploratory, non-production artifacts."
            });
            let citation = json!({
                "citation_id": dynamic_citation_id,
                "citation_usable_for_audit": false,
                "eligibility_status": "waived_for_exploration"
            });
            return Ok(EvidenceEligibilityDecision {
                formula_context: json!({
                    "registry_kind": REGISTRY_KIND,
                    "eligibility_status": "exploratory_unverified",
                    "production_bound": false,
                    "formula": formula,
                    "citation": citation,
                    "warning": "No request-level approval can make this formula production eligible. Ingest and review the source to use production-bound math-to-code."
                }),
                formula,
                citation,
                registry_path: None,
                exploratory_unverified: true,
            });
        }
        return Err(ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.formula_required",
            "Math-to-code requires a formula_record_id/formula_id from the Evidence Eligibility Registry.",
            "Ingest the source document, review formulas/citations, rebuild evidence_registry.json, then call /math-to-code with formula_record_id and evidence_manifest_path. Use allow_exploratory_unverified=true only for non-production exploration."
        ).with_details(json!({"files_written": [], "commands_run": [], "state": "blocked_by_formula_review"})));
    }

    let registry_path = resolve_registry_path(req).await?;
    let registry = read_json_file(&registry_path).await?;
    let formula_id = requested_formula_id.unwrap();
    let formula = find_record(&registry, "formulas", "formula_id", formula_id).ok_or_else(|| {
        ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.formula_not_found",
            format!("Formula `{formula_id}` was not found in the Evidence Eligibility Registry."),
            "Use `veritas-ingest evidence-registry --workspace <run>/ingestion --refresh-from-chunks` and pass one of the returned eligible formula ids."
        ).with_details(json!({"registry_path": registry_path.display().to_string(), "formula_id": formula_id, "state": "blocked_by_formula_review", "files_written": [], "commands_run": []}))
    })?;

    let eligible = formula.get("eligible_for_codegen").and_then(Value::as_bool).unwrap_or(false)
        && formula.get("codegen_eligibility_status").and_then(Value::as_str).unwrap_or("") == "eligible";
    if !eligible {
        return Err(ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.formula_not_eligible",
            format!("Formula `{formula_id}` is not eligible for code generation."),
            "Approve or edit the formula through the review workflow; rejected, pending, low-confidence, or citation-missing formulas cannot trigger codegen."
        ).with_details(json!({"registry_path": registry_path.display().to_string(), "formula": formula, "state": "blocked_by_formula_review", "files_written": [], "commands_run": []})));
    }

    let citation_id_owned = req.citation_record_id.clone()
        .or_else(|| formula.get("citation_id").and_then(Value::as_str).map(|s| s.to_string()));
    let citation = if let Some(cid) = citation_id_owned.as_deref() {
        find_record_any(&registry, "citations", &["citation_id", "paper_id", "source_document_id"], cid).unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };
    let citation_ok = citation.get("citation_usable_for_audit").and_then(Value::as_bool).unwrap_or(false)
        || citation.get("eligible_for_planning").and_then(Value::as_bool).unwrap_or(false);
    if !citation_ok {
        return Err(ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.citation_not_eligible",
            "The formula's source citation is not approved for audit-backed code generation.",
            "Approve/edit the citation review first; production-bound math-to-code cannot use rejected or pending citation provenance."
        ).with_details(json!({"registry_path": registry_path.display().to_string(), "formula": formula, "citation": citation, "state": "blocked_by_citation_review", "files_written": [], "commands_run": []})));
    }

    Ok(EvidenceEligibilityDecision {
        formula_context: json!({
            "registry_kind": REGISTRY_KIND,
            "eligibility_status": "eligible",
            "production_bound": true,
            "registry_path": registry_path.display().to_string(),
            "formula": formula,
            "citation": citation,
            "planning": registry.get("planning").cloned().unwrap_or_else(|| json!({})),
            "summary": registry.get("summary").cloned().unwrap_or_else(|| json!({}))
        }),
        formula,
        citation,
        registry_path: Some(registry_path),
        exploratory_unverified: false,
    })
}


pub(crate) async fn require_planning_eligible(workspace: &Path) -> Result<Value, ApiFailure> {
    let gate = planning_gate_from_workspace(workspace).await?;
    if gate.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(gate);
    }
    Err(ApiFailure::new(
        StatusCode::CONFLICT,
        "evidence_registry.planning_not_eligible",
        "Evidence is not eligible for production-bound planning.",
        "Review citations/formulas and rebuild evidence_registry.json before planning or code generation."
    ).with_details(gate))
}
pub(crate) async fn planning_gate_from_workspace(workspace: &Path) -> Result<Value, ApiFailure> {
    let registry_path = workspace.join("evidence_registry.json");
    if !registry_path.exists() {
        return Ok(json!({"ok": false, "status": "awaiting_evidence_review", "reason": "evidence_registry.json is missing", "registry_path": registry_path.display().to_string()}));
    }
    let registry = read_json_file(&registry_path).await?;
    let planning = registry.get("planning").cloned().unwrap_or_else(|| json!({}));
    let allowed = planning.get("allowed").and_then(Value::as_bool).unwrap_or(false);
    Ok(json!({
        "ok": allowed,
        "status": if allowed { "eligible_for_evidence_backed_planning" } else { planning.get("status").and_then(Value::as_str).unwrap_or("awaiting_evidence_review") },
        "planning": planning,
        "registry_path": registry_path.display().to_string(),
        "next_action": if allowed { "Continue to planning." } else { "Review citations/formulas and rebuild the Evidence Eligibility Registry before planning/codegen." }
    }))
}


pub(crate) async fn registry_status_payload(path: Option<&str>) -> Value {
    let Some(path_value) = path.filter(|value| !value.trim().is_empty()) else {
        return json!({
            "ok": false,
            "status": "path_required",
            "message": "Provide path to evidence_manifest.json or evidence_registry.json."
        });
    };
    let path = PathBuf::from(path_value);
    let registry_path = if path.file_name().and_then(|s| s.to_str()) == Some("evidence_registry.json") {
        path
    } else if path.file_name().and_then(|s| s.to_str()) == Some("evidence_manifest.json") {
        match tokio::fs::read_to_string(&path).await.ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()).and_then(|manifest| manifest.get("evidence_registry_path").and_then(Value::as_str).map(PathBuf::from)) {
            Some(value) => value,
            None => path.parent().map(|p| p.join("evidence_registry.json")).unwrap_or_else(|| PathBuf::from("evidence_registry.json")),
        }
    } else if path.is_dir() {
        path.join("evidence_registry.json")
    } else {
        path
    };
    match read_json_file(&registry_path).await {
        Ok(registry) => json!({
            "ok": true,
            "status": "loaded",
            "registry_path": registry_path.display().to_string(),
            "planning": registry.get("planning").cloned().unwrap_or_else(|| json!({})),
            "summary": registry.get("summary").cloned().unwrap_or_else(|| json!({})),
            "codegen": registry.get("codegen").cloned().unwrap_or_else(|| json!({})),
        }),
        Err(error) => json!({
            "ok": false,
            "status": "unavailable",
            "registry_path": registry_path.display().to_string(),
            "code": error.code,
            "message": error.message,
            "remediation": error.remediation,
        }),
    }
}
async fn resolve_registry_path(req: &MathToCodeRequest) -> Result<PathBuf, ApiFailure> {
    if let Some(path) = req.evidence_registry_path.as_ref().filter(|p| !p.trim().is_empty()) {
        return ensure_registry_path(PathBuf::from(path)).await;
    }
    if let Some(path) = req.evidence_manifest_path.as_ref().filter(|p| !p.trim().is_empty()) {
        let manifest_path = PathBuf::from(path);
        if manifest_path.file_name().and_then(|s| s.to_str()) == Some("evidence_registry.json") {
            return ensure_registry_path(manifest_path).await;
        }
        let manifest = read_json_file(&manifest_path).await?;
        if let Some(registry_path) = manifest.get("evidence_registry_path").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            return ensure_registry_path(PathBuf::from(registry_path)).await;
        }
        if let Some(parent) = manifest_path.parent() {
            return ensure_registry_path(parent.join("evidence_registry.json")).await;
        }
    }
    if let Ok(path) = std::env::var("VERITAS_EVIDENCE_REGISTRY_PATH") {
        if !path.trim().is_empty() {
            return ensure_registry_path(PathBuf::from(path)).await;
        }
    }
    Err(ApiFailure::new(
        StatusCode::CONFLICT,
        "evidence_registry.path_required",
        "No Evidence Eligibility Registry path was provided.",
        "Pass evidence_manifest_path or evidence_registry_path from the ingestion workspace."
    ).with_details(json!({"state": "blocked_by_formula_review", "files_written": [], "commands_run": []})))
}

async fn ensure_registry_path(path: PathBuf) -> Result<PathBuf, ApiFailure> {
    if !path.exists() {
        return Err(ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.not_found",
            format!("Evidence Eligibility Registry not found: {}", path.display()),
            "Build it with `veritas-ingest evidence-registry --workspace <ingestion-workspace> --refresh-from-chunks`."
        ).with_details(json!({"registry_path": path.display().to_string(), "state": "blocked_by_formula_review", "files_written": [], "commands_run": []})));
    }
    Ok(path)
}

async fn read_json_file(path: &Path) -> Result<Value, ApiFailure> {
    let text = tokio::fs::read_to_string(path).await.map_err(|error| {
        ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.read_failed",
            format!("Could not read {}: {error}", path.display()),
            "Verify the run workspace and registry file permissions."
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        ApiFailure::new(
            StatusCode::CONFLICT,
            "evidence_registry.invalid_json",
            format!("{} is not valid JSON: {error}", path.display()),
            "Rebuild the evidence registry from ingestion artifacts."
        )
    })
}

fn find_record(registry: &Value, array_name: &str, key: &str, value: &str) -> Option<Value> {
    registry.get(array_name)?.as_array()?.iter()
        .find(|record| record.get(key).and_then(Value::as_str).map(|s| s == value).unwrap_or(false))
        .cloned()
}

fn find_record_any(registry: &Value, array_name: &str, keys: &[&str], value: &str) -> Option<Value> {
    registry.get(array_name)?.as_array()?.iter()
        .find(|record| keys.iter().any(|key| record.get(*key).and_then(Value::as_str).map(|s| s == value).unwrap_or(false)))
        .cloned()
}
