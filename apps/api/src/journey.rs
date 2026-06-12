use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, path::PathBuf, sync::Arc};
use tokio::{fs, process::Command};

use super::*;

/// The real user-facing journey request.  Phase 1 intentionally makes this
/// the single API surface for the end-user workflow; later phases attach real
/// local ingestion, evidence registry gates, math tools, and artifact decisions.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct JourneyRunRequest {
    pub source: Option<String>,
    pub mode: Option<String>,
    pub goal: Option<String>,
    pub language: Option<String>,
    pub policy: Option<String>,
    pub size: Option<u32>,
    pub max_retries: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JourneyPath {
    pub run_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JourneyReviewRequest {
    pub phase: String,
    pub decision: String,
    pub artifact: Option<Value>,
    pub reviewer: Option<String>,
    pub notes: Option<String>,
    pub policy: Option<String>,
    pub required: Option<bool>,
}

pub(crate) async fn run(State(state): State<Arc<AppState>>, Json(req): Json<JourneyRunRequest>) -> impl IntoResponse {
    match run_inner(&state, req).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => error.response(),
    }
}

pub(crate) async fn status(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<JourneyPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    if !workspace.exists() {
        return ApiFailure::new(
            StatusCode::NOT_FOUND,
            "journey.not_found",
            format!("Journey {} was not found in {}.", path.run_id, state.runs_dir.display()),
            "Run `veritas journey run ...` first or verify VERITAS_RUNS_DIR.",
        ).response();
    }
    let payload = json!({
        "ok": true,
        "kind": "VeritasJourneyStatus",
        "run_id": path.run_id.clone(),
        "workspace": workspace.display().to_string(),
        "journey_state": read_json_file(&workspace.join("journey_state.json")).await,
        "journey_report": read_json_file(&workspace.join("journey_report.json")).await,
        "source_manifest": read_json_file(&workspace.join("source_manifest.json")).await,
        "evidence_manifest": read_json_file(&workspace.join("evidence_manifest.json")).await,
        "formula_manifest": read_json_file(&workspace.join("formula_manifest.json")).await,
        "citation_manifest": read_json_file(&workspace.join("citation_manifest.json")).await,
        "review_queue": read_json_file(&workspace.join("review_queue.json")).await,
        "evidence_registry": read_json_file(&workspace.join("evidence_registry.json")).await,
        "state": read_json_file(&workspace.join("state.json")).await,
        "final_report": read_json_file(&workspace.join("final_report.json")).await,
        "human_checkpoint_gate": human_checkpoint_gate_summary(&workspace, &state.human_loop_policy).await,
        "events_tail": read_events_tail(&workspace.join("events.jsonl"), 50).await.unwrap_or_default(),
        "journey_events_tail": read_events_tail(&workspace.join("journey_lifecycle.jsonl"), 50).await.unwrap_or_default(),
    });
    (StatusCode::OK, Json(payload))
}

pub(crate) async fn resume(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<JourneyPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    match resume_autonomous_run(&state, &path.run_id, workspace).await {
        Ok(report) => {
            let _ = persist_journey_event(&state.runs_dir.join(&path.run_id), "JourneyResumed", json!({"run_id": path.run_id.clone(), "report_status": report.get("final_status")})).await;
            (StatusCode::OK, Json(json!({"ok": true, "kind": "VeritasJourneyResume", "run_id": path.run_id.clone(), "report": report})))
        }
        Err(error) => error.response(),
    }
}

pub(crate) async fn report(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<JourneyPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    if !workspace.exists() {
        return ApiFailure::new(
            StatusCode::NOT_FOUND,
            "journey.report.not_found",
            format!("Journey {} was not found.", path.run_id),
            "Run `veritas journey status <run_id>` to verify the configured runs directory.",
        ).response();
    }
    let report = json!({
        "ok": true,
        "kind": "VeritasJourneyReportBundle",
        "run_id": path.run_id.clone(),
        "workspace": workspace.display().to_string(),
        "journey_report": read_json_file(&workspace.join("journey_report.json")).await,
        "final_report": read_json_file(&workspace.join("final_report.json")).await,
        "source_manifest": read_json_file(&workspace.join("source_manifest.json")).await,
        "evidence_manifest": read_json_file(&workspace.join("evidence_manifest.json")).await,
        "formula_manifest": read_json_file(&workspace.join("formula_manifest.json")).await,
        "citation_manifest": read_json_file(&workspace.join("citation_manifest.json")).await,
        "review_queue": read_json_file(&workspace.join("review_queue.json")).await,
        "evidence_registry": read_json_file(&workspace.join("evidence_registry.json")).await,
        "ingestion_report_markdown": read_text_file(&workspace.join("ingestion_report.md")).await,
        "request": read_json_file(&workspace.join("request.json")).await,
        "events": read_events_tail(&workspace.join("events.jsonl"), 1000).await.unwrap_or_default(),
        "journey_events": read_events_tail(&workspace.join("journey_lifecycle.jsonl"), 1000).await.unwrap_or_default(),
        "human_checkpoints": read_events_tail(&workspace.join("human_checkpoints.jsonl"), 1000).await.unwrap_or_default(),
    });
    (StatusCode::OK, Json(report))
}

pub(crate) async fn review(
    State(state): State<Arc<AppState>>,
    AxumPath(path): AxumPath<JourneyPath>,
    Json(req): Json<JourneyReviewRequest>,
) -> impl IntoResponse {
    match record_journey_review(&state, &path.run_id, req).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => error.response(),
    }
}

async fn run_inner(state: &AppState, req: JourneyRunRequest) -> Result<Value, ApiFailure> {
    let goal = req.goal.clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match req.source.as_deref() {
            Some(source) => format!("Transform source document {source} into governed, validated software artifacts."),
            None => "Run a governed Veritas research-to-code workflow.".to_string(),
        });
    if goal.trim().is_empty() {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "journey.validation.goal", "Journey goal is empty.", "Provide --goal or a source document."));
    }
    fs::create_dir_all(&state.runs_dir).await.map_err(|error| ApiFailure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "journey.runs_dir_create",
        format!("Could not create runs directory: {error}"),
        "Ensure the API container has write access to VERITAS_RUNS_DIR.",
    ))?;
    let run_id = format!("journey-{}-{}", now_millis(), uuid::Uuid::new_v4().simple());
    let workspace = state.runs_dir.join(&run_id);
    fs::create_dir_all(&workspace).await.map_err(|error| ApiFailure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "journey.workspace_create",
        format!("Could not create journey workspace: {error}"),
        "Ensure the API container has write access to VERITAS_RUNS_DIR.",
    ))?;
    let _lock = acquire_run_lock(&workspace, &run_id).await?;
    let mode = req.mode.clone().unwrap_or_else(|| "local".to_string());
    let policy = req.policy.clone().unwrap_or_else(|| state.human_loop_policy.clone());
    let language = req.language.clone().unwrap_or_else(|| "rust".to_string());
    let mut source_manifest = build_source_manifest(req.source.clone(), &mode).await;
    write_json_file(&workspace.join("source_manifest.json"), &source_manifest).await?;
    write_json_file(&workspace.join("journey_request.json"), &json!({
        "kind": "VeritasJourneyRequest",
        "run_id": run_id.clone(),
        "source": req.source.clone(),
        "mode": mode.clone(),
        "goal": goal.clone(),
        "language": language.clone(),
        "policy": policy.clone(),
        "size": req.size,
        "max_retries": req.max_retries,
        "created_at_ms": now_millis()
    })).await?;
    let run_req = RunRequest {
        goal: enrich_goal_with_journey_context(&goal, &source_manifest, &policy, &mode),
        language: Some(language.clone()),
        size: req.size,
        max_retries: req.max_retries,
    };
    let mut persisted_req: RunResumeRequest = run_req.clone().into();
    persisted_req.run_id = run_id.clone();
    write_json_file(&workspace.join("request.json"), &json!(persisted_req)).await?;
    persist_run_state(&workspace, "JourneyCreated", json!({"goal": goal.clone(), "run_id": run_id.clone(), "mode": mode.clone(), "source_manifest": source_manifest.clone()})).await?;
    persist_journey_event(&workspace, "SourceRegistered", json!({"source_manifest": source_manifest.clone()})).await?;

    let ingestion_report = if req.source.is_some() && mode.eq_ignore_ascii_case("local") {
        let ingestion = run_local_ingestion(&workspace, &req, state).await?;
        source_manifest = merge_source_manifest_ingestion(source_manifest, &ingestion);
        write_json_file(&workspace.join("source_manifest.json"), &source_manifest).await?;
        persist_run_state(&workspace, "JourneyIngested", json!({"source_manifest": source_manifest.clone(), "ingestion": ingestion.clone()})).await?;
        persist_journey_event(&workspace, "LocalIngestionCompleted", json!({"ingestion": ingestion.clone()})).await?;
        Some(ingestion)
    } else {
        None
    };

    if let Some(ingestion) = &ingestion_report {
        if !ingestion.get("retrieval_status").and_then(|value| value.get("available")).and_then(Value::as_bool).unwrap_or(false) {
            let journey_report = json!({
                "ok": false,
                "kind": "VeritasJourneyRunReport",
                "run_id": run_id.clone(),
                "workspace": workspace.display().to_string(),
                "mode": mode.clone(),
                "source_manifest": source_manifest.clone(),
                "ingestion": ingestion.clone(),
                "policy": policy.clone(),
                "real_product_path": true,
                "mocked": false,
                "phase": "phase4_pre_execution_gate_engine",
                "state": "blocked_by_retrieval_unavailable",
                "blocked_stage": "local_ingestion_retrieval",
                "final_status": "blocked_by_retrieval_unavailable",
                "files_written": [],
                "commands_run": [],
                "next_action": "Install sentence-transformers locally, configure the embedding service, or rerun with a production embedding backend before planning/codegen.",
            });
            write_json_file(&workspace.join("journey_report.json"), &journey_report).await?;
            write_json_file(&workspace.join("final_report.json"), &journey_report).await?;
            persist_journey_event(&workspace, "JourneyBlockedByRetrievalUnavailable", journey_report.clone()).await?;
            return Ok(json!({"ok": false, "journey": journey_report}));
        }
    }

    if let Some(_ingestion) = &ingestion_report {
        match evidence_registry::require_planning_eligible(&workspace).await {
            Ok(registry_gate) => {
                write_json_file(&workspace.join("evidence_gate.json"), &registry_gate).await?;
                persist_journey_event(&workspace, "EvidenceEligibilityPassed", registry_gate).await?;
            }
            Err(error) => {
                let journey_report = json!({
                    "ok": false,
                    "kind": "VeritasJourneyRunReport",
                    "run_id": run_id.clone(),
                    "workspace": workspace.display().to_string(),
                    "mode": mode.clone(),
                    "source_manifest": source_manifest.clone(),
                    "policy": policy.clone(),
                    "real_product_path": true,
                    "mocked": false,
                    "phase": "phase4_pre_execution_gate_engine",
                    "state": "awaiting_evidence_review",
                    "blocked_stage": "evidence_eligibility_registry",
                    "final_status": "awaiting_evidence_review",
                    "files_written": [],
                    "commands_run": [],
                    "error": {"code": error.code, "message": error.message, "remediation": error.remediation, "details": error.details},
                    "next_action": "Review citations/formulas, rebuild evidence_registry.json, then resume the journey."
                });
                write_json_file(&workspace.join("journey_report.json"), &journey_report).await?;
                write_json_file(&workspace.join("final_report.json"), &journey_report).await?;
                persist_journey_event(&workspace, "JourneyBlockedByEvidenceEligibility", journey_report.clone()).await?;
                return Ok(json!({"ok": false, "journey": journey_report}));
            }
        }
    }

    if ingestion_report.is_some() {
        let planning_gate = evidence_registry::planning_gate_from_workspace(&workspace).await?;
        write_json_file(&workspace.join("evidence_planning_gate.json"), &planning_gate).await?;
        if !planning_gate.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            let journey_report = json!({
                "ok": false,
                "kind": "VeritasJourneyRunReport",
                "run_id": run_id.clone(),
                "workspace": workspace.display().to_string(),
                "mode": mode.clone(),
                "source_manifest": source_manifest.clone(),
                "ingestion": ingestion_report.clone(),
                "policy": policy.clone(),
                "real_product_path": true,
                "mocked": false,
                "phase": "phase4_pre_execution_gate_engine",
                "state": "awaiting_evidence_review",
                "blocked_stage": "evidence_eligibility_registry",
                "final_status": "awaiting_evidence_review",
                "files_written": [],
                "commands_run": [],
                "evidence_planning_gate": planning_gate,
                "next_action": "Review citations/formulas, rebuild evidence_registry.json, then resume the journey."
            });
            write_json_file(&workspace.join("journey_report.json"), &journey_report).await?;
            write_json_file(&workspace.join("final_report.json"), &journey_report).await?;
            persist_journey_event(&workspace, "JourneyBlockedByEvidenceReview", journey_report.clone()).await?;
            return Ok(json!({"ok": false, "journey": journey_report}));
        }
        persist_journey_event(&workspace, "EvidenceEligibilityRegistryPassed", planning_gate).await?;
    }

    let final_report = execute_autonomous_run_core(state, run_id.clone(), workspace.clone(), run_req, false).await?;
    let journey_report = json!({
        "ok": final_report.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "kind": "VeritasJourneyRunReport",
        "run_id": run_id.clone(),
        "workspace": workspace.display().to_string(),
        "mode": mode.clone(),
        "source_manifest": source_manifest.clone(),
        "policy": policy.clone(),
        "delegated_to": "execute_autonomous_run_core",
        "real_product_path": true,
        "mocked": false,
        "phase": "phase4_pre_execution_gate_engine",
        "ingestion": ingestion_report.clone(),
        "final_report_path": "final_report.json",
        "final_status": final_report.get("final_status").cloned().unwrap_or_else(|| json!("unknown")),
        "next_product_phases": [
            "tool_verified_math_engine",
            "artifact_decision_engine",
            "behavior_derived_scorecards"
        ]
    });
    write_json_file(&workspace.join("journey_report.json"), &journey_report).await?;
    persist_journey_event(&workspace, "JourneyCompleted", json!({"journey_report": journey_report.clone(), "final_report_status": final_report.get("final_status")})).await?;
    Ok(json!({"ok": true, "journey": journey_report, "report": final_report}))
}

async fn build_source_manifest(source: Option<String>, mode: &str) -> Value {
    match source {
        Some(source_value) => {
            let source_path = PathBuf::from(&source_value);
            let exists = source_path.exists();
            json!({
                "kind": "VeritasSourceManifest",
                "source": source_value,
                "mode": mode,
                "exists_on_api_host": exists,
                "status": if exists { "registered" } else { "registered_unverified_path" },
                "ingestion_status": "registered_for_local_ingestion",
                "ingestion_backend": if mode.eq_ignore_ascii_case("local") { "local" } else { "opensearch_fuseki" },
                "message": "Source is registered for the real journey orchestrator. Local mode invokes the real local ingestion backend before planning/codegen."
            })
        }
        None => json!({
            "kind": "VeritasSourceManifest",
            "source": null,
            "mode": mode,
            "status": "not_provided",
            "ingestion_status": "not_required_for_prompt_only_journey"
        })
    }
}


fn merge_source_manifest_ingestion(mut source_manifest: Value, ingestion: &Value) -> Value {
    if let Some(obj) = source_manifest.as_object_mut() {
        obj.insert("ingestion_status".to_string(), json!("completed"));
        obj.insert("ingestion_backend".to_string(), json!("local"));
        obj.insert("ingestion_output_dir".to_string(), ingestion.get("output_dir").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("evidence_manifest_path".to_string(), ingestion.get("evidence_manifest_path").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("formula_manifest_path".to_string(), ingestion.get("formula_manifest_path").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("citation_manifest_path".to_string(), ingestion.get("citation_manifest_path").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("review_queue_path".to_string(), ingestion.get("review_queue_path").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("evidence_registry_path".to_string(), ingestion.get("evidence_registry_path").cloned().unwrap_or_else(|| json!(null)));
        obj.insert("retrieval_status".to_string(), ingestion.get("retrieval_status").cloned().unwrap_or_else(|| json!({"available": false})));
    }
    source_manifest
}

async fn run_local_ingestion(workspace: &std::path::Path, req: &JourneyRunRequest, _state: &AppState) -> Result<Value, ApiFailure> {
    let source = req.source.clone().ok_or_else(|| ApiFailure::new(
        StatusCode::BAD_REQUEST,
        "journey.ingestion.source_required",
        "Local journey ingestion requires a source PDF path.",
        "Pass --source path/to/paper.pdf.",
    ))?;
    let source_path = PathBuf::from(&source);
    if !source_path.exists() {
        return Err(ApiFailure::new(
            StatusCode::BAD_REQUEST,
            "journey.ingestion.source_missing",
            format!("Source file does not exist on the API host: {source}"),
            "Use a path visible to the API process/container, or mount the source directory into the API container.",
        ));
    }
    let ingestion_dir = workspace.join("ingestion");
    fs::create_dir_all(&ingestion_dir).await.map_err(|error| ApiFailure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "journey.ingestion.workspace_create",
        format!("Could not create ingestion workspace: {error}"),
        "Ensure VERITAS_RUNS_DIR is writable.",
    ))?;
    let python = env::var("VERITAS_INGEST_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let config_path = env::var("VERITAS_CONFIG").unwrap_or_else(|_| "config/veritas.yaml".to_string());
    let pythonpath = env::var("VERITAS_INGEST_PYTHONPATH").unwrap_or_else(|_| "services/ingestion:/workspace/services/ingestion:/app".to_string());
    let mut cmd = Command::new(&python);
    cmd.env("PYTHONPATH", pythonpath)
        .env("VERITAS_CONFIG", &config_path)
        .current_dir(env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .arg("-m")
        .arg("veritas_ingest.cli")
        .arg("--config")
        .arg(&config_path)
        .arg("ingest-pdf")
        .arg("--path")
        .arg(&source)
        .arg("--paper-id")
        .arg(source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("local_document"))
        .arg("--backend")
        .arg("local")
        .arg("--workspace")
        .arg(ingestion_dir.display().to_string());
    let output = cmd.output().await.map_err(|error| ApiFailure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "journey.ingestion.command_spawn",
        format!("Could not start local ingestion command `{python}`: {error}"),
        "Install Python 3 and the Veritas ingestion package, or use the Docker ingestion service.",
    ))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    write_json_file(&workspace.join("local_ingestion_command.json"), &json!({
        "python": python,
        "config": config_path,
        "source": source,
        "workspace": ingestion_dir.display().to_string(),
        "exit_code": output.status.code(),
        "stdout_tail": stdout.chars().rev().take(4000).collect::<String>().chars().rev().collect::<String>(),
        "stderr_tail": stderr.chars().rev().take(4000).collect::<String>().chars().rev().collect::<String>(),
    })).await?;
    if !output.status.success() {
        return Err(ApiFailure::new(
            StatusCode::BAD_GATEWAY,
            "journey.ingestion.command_failed",
            format!("Local ingestion failed with status {:?}: {}", output.status.code(), stderr.lines().last().unwrap_or("see local_ingestion_command.json")),
            "Open local_ingestion_command.json and ingestion logs; verify the PDF is readable and local ingestion dependencies are installed.",
        ));
    }
    let payload = extract_last_json_object(&stdout).ok_or_else(|| ApiFailure::new(
        StatusCode::BAD_GATEWAY,
        "journey.ingestion.no_json",
        "Local ingestion completed but did not emit a JSON summary.",
        "Inspect local_ingestion_command.json and rerun ingestion.",
    ))?;
    write_json_file(&workspace.join("local_ingestion_result.json"), &payload).await?;
    promote_local_ingestion_artifacts(workspace, &ingestion_dir).await?;
    let output_value = payload.get("output").cloned().unwrap_or_else(|| payload.clone());
    Ok(output_value)
}

async fn promote_local_ingestion_artifacts(workspace: &std::path::Path, ingestion_dir: &std::path::Path) -> Result<(), ApiFailure> {
    for name in [
        "evidence_manifest.json",
        "formula_manifest.json",
        "citation_manifest.json",
        "review_queue.json",
        "evidence_registry.json",
        "evidence_eligibility.json",
        "ingestion_report.md",
        "evidence.ttl",
        "chunks.jsonl",
        "formulas.jsonl",
        "citations.jsonl",
        "local_vector_index.jsonl",
        "local_lexical_index.jsonl",
    ] {
        let src = ingestion_dir.join(name);
        if src.exists() {
            fs::copy(&src, workspace.join(name)).await.map_err(|error| ApiFailure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "journey.ingestion.promote_artifact",
                format!("Could not copy local ingestion artifact {name}: {error}"),
                "Check run workspace permissions and retry.",
            ))?;
        }
    }
    Ok(())
}

async fn read_text_file(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path).await.ok()
}

fn extract_last_json_object(stdout: &str) -> Option<Value> {
    for index in stdout.match_indices('{').map(|(idx, _)| idx).rev() {
        if let Ok(value) = serde_json::from_str::<Value>(&stdout[index..]) {
            return Some(value);
        }
    }
    None
}

fn enrich_goal_with_journey_context(goal: &str, source_manifest: &Value, policy: &str, mode: &str) -> String {
    format!(
        "{goal}\n\nVeritas Journey Context:\nmode={mode}\nhuman_checkpoint_policy={policy}\nsource_manifest={}\nUse evidence, ontology facts, validation gates, and final reports. Do not claim production readiness unless validation and governance gates pass.",
        source_manifest
    )
}

async fn persist_journey_event(workspace: &std::path::Path, state_name: &str, payload: Value) -> Result<(), ApiFailure> {
    let sequence = next_event_sequence(&workspace.join("journey_lifecycle.jsonl")).await;
    let event = json!({
        "ts_ms": now_millis(),
        "sequence": sequence,
        "state": state_name,
        "payload": payload,
    });
    write_json_file(&workspace.join("journey_state.json"), &event).await?;
    append_jsonl(&workspace.join("journey_lifecycle.jsonl"), &event).await
}

async fn record_journey_review(state: &AppState, run_id: &str, req: JourneyReviewRequest) -> Result<Value, ApiFailure> {
    let decision = req.decision.trim().to_ascii_lowercase();
    if !["pending", "approve", "edit", "reject", "skip", "auto_approve", "ask_for_explanation"].contains(&decision.as_str()) {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "journey.review.invalid_decision", format!("Unsupported decision: {}", req.decision), "Use pending, approve, edit, reject, skip, auto_approve, or ask_for_explanation."));
    }
    let phase = req.phase.trim().to_ascii_lowercase();
    let allowed_phases = ["citation_review", "formula_review", "representation_review", "plan_review", "code_architecture_review", "validation_review", "math_to_code_representation_review"];
    if !allowed_phases.contains(&phase.as_str()) {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "journey.review.invalid_phase", format!("Unsupported checkpoint phase: {}", req.phase), "Use citation_review, formula_review, representation_review, plan_review, code_architecture_review, validation_review, or math_to_code_representation_review."));
    }
    let workspace = state.runs_dir.join(run_id);
    if !workspace.exists() {
        return Err(ApiFailure::new(StatusCode::NOT_FOUND, "journey.review.not_found", format!("Journey {run_id} was not found."), "Run `veritas journey status <run_id>` to verify the run id."));
    }
    let policy = req.policy.unwrap_or_else(|| state.human_loop_policy.clone());
    let notes = req.notes.unwrap_or_default();
    let artifact_value = req.artifact.unwrap_or_else(|| json!({}));
    let required = req.required.unwrap_or_else(|| human_checkpoint_required(&policy, &phase, &artifact_value));
    let approved = human_decision_approved(&decision, &notes) || (!required && decision != "reject");
    let blocked = human_decision_blocks(&decision, required, &notes);
    let checkpoint = json!({
        "kind": "HumanCheckpoint",
        "run_id": run_id,
        "phase": phase,
        "policy": policy,
        "required": required,
        "decision": decision,
        "approved": approved,
        "blocked": blocked,
        "waived": decision == "skip" && !notes.trim().is_empty(),
        "reviewer": req.reviewer.unwrap_or_else(|| "human".to_string()),
        "notes": notes,
        "artifact": artifact_value,
        "timestamp_ms": now_millis(),
        "status": if approved { "approved_or_waived" } else if blocked { "blocked" } else { "recorded" },
        "recorded_by": "journey.review"
    });
    append_jsonl(&workspace.join("human_checkpoints.jsonl"), &checkpoint).await?;
    persist_run_state(&workspace, "JourneyHumanCheckpointRecorded", checkpoint.clone()).await?;
    persist_journey_event(&workspace, "HumanCheckpointRecorded", checkpoint.clone()).await?;
    let gate = human_checkpoint_gate_summary(&workspace, &state.human_loop_policy).await;
    Ok(json!({"ok": true, "kind": "VeritasJourneyReview", "run_id": run_id, "checkpoint": checkpoint, "human_checkpoint_gate": gate, "workspace": workspace.display().to_string()}))
}
