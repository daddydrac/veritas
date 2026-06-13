mod providers;
mod schemas;
mod journey;
mod evidence_registry;
mod gates;
mod math_tools;
mod tools;
mod governance;
mod artifact_decision;
mod lineage;
mod planning_context;

use axum::{
    extract::{State, Path as AxumPath},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use providers::{ModelRole, ProviderError, ProviderRouter};
use governance::GovernanceMode;
use schemas::{SchemaKey, schema_json, validate_json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{fs, process::Command, sync::Mutex, time::timeout};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub(crate) struct AppState {
    http: Client,
    provider_router: ProviderRouter,
    opensearch_url: String,
    opensearch_index: String,
    opensearch_base_index: String,
    opensearch_read_alias: String,
    opensearch_write_alias: String,
    opensearch_versioned_index: String,
    opensearch_mapping_version: String,
    opensearch_vector_field: String,
    opensearch_vector_dimension: usize,
    fuseki_query_url: String,
    fuseki_ping_url: String,
    embedding_url: String,
    shacl_url: String,
    math_tools_url: String,
    math_tools_timeout_secs: u64,
    require_models: bool,
    planner_model: ModelRole,
    code_model: ModelRole,
    math_model: ModelRole,
    code_fallback_model: String,
    math_large_model: String,
    remote_model_enabled: bool,
    remote_model_base_url: String,
    remote_model_name: String,
    remote_model_api_key_env: String,
    command_runner: String,
    governance_mode: GovernanceMode,
    human_loop_policy: String,
    fuseki_data_url: String,
    graph_ontology_uri: String,
    graph_document_base_uri: String,
    graph_run_base_uri: String,
    graph_validation_base_uri: String,
    runs_dir: PathBuf,
    command_timeout_secs: u64,
    max_retries: usize,
    recent_runs: Arc<Mutex<Vec<Value>>>,
}

#[derive(Debug, Deserialize)]
struct SparqlRequest {
    query: String,
}

#[derive(Debug, Deserialize)]
struct SearchRequest {
    index: Option<String>,
    query: String,
    size: Option<u32>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenSearchMigrateRequest {
    dry_run: Option<bool>,
    force_alias_update: Option<bool>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphUploadRequest {
    graph_uri: Option<String>,
    turtle: String,
    replace: Option<bool>,
    content_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphDescribeRequest {
    graph_uri: String,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct RunRequest {
    goal: String,
    language: Option<String>,
    size: Option<u32>,
    max_retries: Option<usize>,
    execution_mode: Option<String>,
    #[serde(default)]
    preloaded_artifacts: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Health {
    service: &'static str,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MathToCodeRequest {
    pub(crate) formula_id: Option<String>,
    pub(crate) formula_record_id: Option<String>,
    pub(crate) citation_record_id: Option<String>,
    pub(crate) evidence_manifest_path: Option<String>,
    pub(crate) evidence_registry_path: Option<String>,
    pub(crate) review_decision_id: Option<String>,
    pub(crate) allow_exploratory_unverified: Option<bool>,
    pub(crate) formula_latex: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) max_retries: Option<usize>,
    pub(crate) human_decision: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct EvidenceRegistryStatusRequest {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MathToolsValidateRequest {
    workspace: Option<String>,
    goal: Option<String>,
    formula_latex: Option<String>,
    formula_id: Option<String>,
    assumptions: Option<Vec<String>>,
    variables: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct HumanCheckpointRequest {
    run_id: Option<String>,
    phase: String,
    decision: String,
    artifact: Option<Value>,
    reviewer: Option<String>,
    notes: Option<String>,
    policy: Option<String>,
    required: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RunPath {
    run_id: String,
}

#[derive(Debug)]
struct ApiFailure {
    status: StatusCode,
    code: String,
    message: String,
    remediation: String,
    details: Value,
}


#[derive(Debug)]
struct RunLock {
    path: PathBuf,
    run_id: String,
}

impl Drop for RunLock {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(run_id = %self.run_id, lock = %self.path.display(), error = %error, "failed to remove run lock");
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct RunResumeRequest {
    run_id: String,
    goal: String,
    language: Option<String>,
    size: Option<u32>,
    max_retries: Option<usize>,
    execution_mode: Option<String>,
    #[serde(default)]
    preloaded_artifacts: Option<Value>,
}

impl From<RunRequest> for RunResumeRequest {
    fn from(value: RunRequest) -> Self {
        Self {
            run_id: String::new(),
            goal: value.goal,
            language: value.language,
            size: value.size,
            max_retries: value.max_retries,
            execution_mode: value.execution_mode,
            preloaded_artifacts: value.preloaded_artifacts,
        }
    }
}

impl RunResumeRequest {
    fn into_run_request(self) -> RunRequest {
        RunRequest {
            goal: self.goal,
            language: self.language,
            size: self.size,
            max_retries: self.max_retries,
            execution_mode: self.execution_mode,
            preloaded_artifacts: self.preloaded_artifacts,
        }
    }
}

impl ApiFailure {
    fn new(status: StatusCode, code: &str, message: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            status,
            code: code.to_string(),
            message: message.into(),
            remediation: remediation.into(),
            details: json!({}),
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    fn from_provider_error(error: ProviderError) -> Self {
        let status = match error.code.as_str() {
            "model.timeout" => StatusCode::GATEWAY_TIMEOUT,
            "remote.disabled" => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::BAD_GATEWAY,
        };
        ApiFailure::new(status, &error.code, error.message.clone(), error.remediation.clone())
            .with_details(json!({
                "provider": error.provider,
                "role": error.role,
                "category": error.category,
                "retryable": error.retryable,
                "details": error.details
            }))
    }

    fn response(self) -> (StatusCode, Json<Value>) {
        (
            self.status,
            Json(json!({
                "ok": false,
                "error": {
                    "code": self.code,
                    "message": self.message,
                    "remediation": self.remediation,
                    "details": self.details
                }
            })),
        )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let port: u16 = env::var("VERITAS_API_PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()?;
    let http = Client::new();
    let remote_enabled = bool_env("VERITAS_REMOTE_MODEL_ENABLED", false);
    let remote_base_url = env::var("VERITAS_REMOTE_MODEL_BASE_URL").unwrap_or_default();
    let remote_model_name = env::var("VERITAS_REMOTE_MODEL_NAME").unwrap_or_default();
    let remote_api_key_env = env::var("VERITAS_REMOTE_MODEL_API_KEY_ENV").unwrap_or_else(|_| "VERITAS_REMOTE_MODEL_API_KEY".into());
    let provider_router = ProviderRouter::new(http.clone(), remote_enabled, remote_base_url.clone(), remote_model_name.clone(), remote_api_key_env.clone());
    let opensearch_base_index = env::var("VERITAS_OPENSEARCH_INDEX_BASE")
        .or_else(|_| env::var("VERITAS_OPENSEARCH_INDEX"))
        .unwrap_or_else(|_| "veritas-papers".into());
    let opensearch_mapping_version = env::var("VERITAS_OPENSEARCH_MAPPING_VERSION").unwrap_or_else(|_| "v1".into());
    let opensearch_versioned_index = env::var("VERITAS_OPENSEARCH_VERSIONED_INDEX")
        .unwrap_or_else(|_| format!("{}-{}", opensearch_base_index, opensearch_mapping_version));
    let opensearch_read_alias = env::var("VERITAS_OPENSEARCH_READ_ALIAS")
        .unwrap_or_else(|_| format!("{}-read", opensearch_base_index));
    let opensearch_write_alias = env::var("VERITAS_OPENSEARCH_WRITE_ALIAS")
        .unwrap_or_else(|_| format!("{}-write", opensearch_base_index));
    let opensearch_search_index = env::var("VERITAS_OPENSEARCH_INDEX")
        .unwrap_or_else(|_| opensearch_read_alias.clone());
    let state = Arc::new(AppState {
        http: http.clone(),
        provider_router,
        opensearch_url: env::var("VERITAS_OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://opensearch:9200".into()),
        opensearch_index: opensearch_search_index,
        opensearch_base_index,
        opensearch_read_alias,
        opensearch_write_alias,
        opensearch_versioned_index,
        opensearch_mapping_version,
        opensearch_vector_field: env::var("VERITAS_OPENSEARCH_VECTOR_FIELD")
            .unwrap_or_else(|_| "embedding".into()),
        opensearch_vector_dimension: uint_env("VERITAS_OPENSEARCH_VECTOR_DIMENSION", 768) as usize,
        fuseki_query_url: env::var("VERITAS_FUSEKI_QUERY_URL")
            .unwrap_or_else(|_| "http://fuseki:3030/veritas/sparql".into()),
        fuseki_ping_url: env::var("VERITAS_FUSEKI_PING_URL")
            .unwrap_or_else(|_| "http://fuseki:3030/$/ping".into()),
        embedding_url: env::var("VERITAS_EMBEDDING_URL")
            .unwrap_or_else(|_| "http://embedding:8090".into()),
        shacl_url: env::var("VERITAS_SHACL_URL")
            .unwrap_or_else(|_| "http://shacl:8080".into()),
        math_tools_url: env::var("VERITAS_MATH_TOOLS_URL")
            .unwrap_or_else(|_| "http://math-tools:8091".into()),
        math_tools_timeout_secs: uint_env("VERITAS_MATH_TOOLS_TIMEOUT_SECS", 60) as u64,
        require_models: bool_env("VERITAS_REQUIRE_MODELS", true),
        planner_model: model_role("planner", "VERITAS_PLANNER", "http://vllm-planner:8000", "Qwen/Qwen2.5-Coder-7B-Instruct", "veritas-planner", 0.05, 0.9, 2200),
        code_model: model_role("code_generation", "VERITAS_CODE", "http://vllm-code:8000", "Qwen/Qwen2.5-Coder-14B-Instruct", "veritas-code", 0.02, 0.9, 7000),
        math_model: model_role("math_reasoning", "VERITAS_MATH", "http://vllm-math:8000", "allenai/Olmo-3-7B-Instruct", "veritas-math", 0.05, 0.9, 5000),
        code_fallback_model: env::var("VERITAS_CODE_FALLBACK_MODEL")
            .unwrap_or_else(|_| "deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct".into()),
        math_large_model: env::var("VERITAS_MATH_LARGE_MODEL")
            .unwrap_or_else(|_| "allenai/Olmo-3.1-32B-Instruct".into()),
        remote_model_enabled: remote_enabled,
        remote_model_base_url: remote_base_url,
        remote_model_name,
        remote_model_api_key_env: remote_api_key_env,
        command_runner: env::var("VERITAS_COMMAND_RUNNER").unwrap_or_else(|_| "local".into()),
        governance_mode: GovernanceMode::from_env(),
        human_loop_policy: env::var("VERITAS_HUMAN_LOOP_POLICY").unwrap_or_else(|_| "require_high_risk_only".into()),
        fuseki_data_url: env::var("VERITAS_FUSEKI_DATA_URL").unwrap_or_else(|_| "http://fuseki:3030/veritas/data".into()),
        graph_ontology_uri: env::var("VERITAS_GRAPH_ONTOLOGY_URI").unwrap_or_else(|_| "urn:veritas:graph:ontology".into()),
        graph_document_base_uri: env::var("VERITAS_GRAPH_DOCUMENT_BASE_URI").unwrap_or_else(|_| "urn:veritas:graph:document".into()),
        graph_run_base_uri: env::var("VERITAS_GRAPH_RUN_BASE_URI").unwrap_or_else(|_| "urn:veritas:graph:run".into()),
        graph_validation_base_uri: env::var("VERITAS_GRAPH_VALIDATION_BASE_URI").unwrap_or_else(|_| "urn:veritas:graph:validation".into()),
        runs_dir: PathBuf::from(env::var("VERITAS_RUNS_DIR").unwrap_or_else(|_| "/workspace/data/runs".into())),
        command_timeout_secs: uint_env("VERITAS_COMMAND_TIMEOUT_SECS", 180),
        max_retries: uint_env("VERITAS_AGENT_MAX_RETRIES", 2) as usize,
        recent_runs: Arc::new(Mutex::new(Vec::new())),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/models", get(models))
        .route("/status", get(status))
        .route("/status/:run_id", get(status_by_run_id))
        .route("/run/:run_id/resume", post(resume_run))
        .route("/run/:run_id/cancel", post(cancel_run))
        .route("/graph/status", get(graph_status))
        .route("/graphs", get(graphs))
        .route("/graph/describe", post(graph_describe))
        .route("/graph/upload", post(graph_upload))
        .route("/graph/facts", get(graph_facts))
        .route("/sparql", post(sparql))
        .route("/search", post(search))
        .route("/opensearch/status", get(opensearch_status))
        .route("/opensearch/mapping", get(opensearch_mapping))
        .route("/opensearch/migrate", post(opensearch_migrate))
        .route("/math-to-code", post(math_to_code))
        .route("/human/checkpoint", post(human_checkpoint))
        .route("/journey/run", post(journey::run))
        .route("/journey/:run_id/status", get(journey::status))
        .route("/journey/:run_id/review", post(journey::review))
        .route("/journey/:run_id/resume", post(journey::resume))
        .route("/journey/:run_id/report", get(journey::report))
        .route("/evidence-registry/status", post(evidence_registry_status))
        .route("/math-tools/status", get(math_tools_status))
        .route("/math-tools/validate", post(math_tools_validate))
        .route("/plan", post(plan))
        .route("/run", post(run_agent))
        .route("/llm/chat", post(llm_chat))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Veritas API listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn model_role(role: &'static str, prefix: &str, default_url: &str, default_model: &str, default_served: &str, default_temp: f32, default_top_p: f32, default_max_tokens: u32) -> ModelRole {
    ModelRole {
        role,
        url: env::var(format!("{prefix}_VLLM_URL")).unwrap_or_else(|_| default_url.into()),
        model: env::var(format!("{prefix}_MODEL")).unwrap_or_else(|_| default_model.into()),
        served_model_name: env::var(format!("{prefix}_SERVED_MODEL_NAME")).unwrap_or_else(|_| default_served.into()),
        temperature: float_env(&format!("{prefix}_TEMPERATURE"), default_temp),
        top_p: float_env(&format!("{prefix}_TOP_P"), default_top_p),
        max_tokens: uint_env(&format!("{prefix}_MAX_TOKENS"), default_max_tokens),
        timeout_secs: uint_env(&format!("{prefix}_TIMEOUT_SECS"), 300) as u64,
    }
}

fn bool_env(name: &str, default: bool) -> bool {
    env::var(name)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn float_env(name: &str, default: f32) -> f32 {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn uint_env(name: &str, default: u32) -> u32 {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

async fn health() -> Json<Health> {
    Json(Health {
        service: "veritas-api",
        status: "ok",
    })
}

async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let opensearch = probe(&state.http, &state.opensearch_url, "opensearch").await;
    let fuseki = probe(&state.http, &state.fuseki_ping_url, "fuseki").await;
    let embedding = probe(&state.http, &format!("{}/health", state.embedding_url.trim_end_matches('/')), "embedding").await;
    let shacl = probe(&state.http, &format!("{}/health", state.shacl_url.trim_end_matches('/')), "shacl").await;
    let math_tools = probe(&state.http, &format!("{}/health", state.math_tools_url.trim_end_matches('/')), "math_tools").await;
    let planner = probe_model(&state.http, &state.planner_model).await;
    let code = probe_model(&state.http, &state.code_model).await;
    let math = probe_model(&state.http, &state.math_model).await;
    let base_ok = opensearch["ok"].as_bool().unwrap_or(false)
        && fuseki["ok"].as_bool().unwrap_or(false)
        && embedding["ok"].as_bool().unwrap_or(false)
        && shacl["ok"].as_bool().unwrap_or(false)
        && math_tools["ok"].as_bool().unwrap_or(false);
    let model_ok = planner["ok"].as_bool().unwrap_or(false)
        && code["ok"].as_bool().unwrap_or(false)
        && math["ok"].as_bool().unwrap_or(false);
    let ok = base_ok && (!state.require_models || model_ok);
    let status = if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (
        status,
        Json(json!({
            "service": "veritas-api",
            "ready": ok,
            "model_services_required": state.require_models,
            "checks": {
                "opensearch": opensearch,
                "fuseki": fuseki,
                "embedding": embedding,
                "shacl": shacl,
                "math_tools": math_tools,
                "vllm_planner": planner,
                "vllm_code": code,
                "vllm_math": math
            },
            "help": if ok { "Required services are reachable." } else { "Run `docker compose ps` and `docker compose logs --tail=200`; for local models run `docker compose --profile models --profile code-model --profile math-model up -d`." }
        })),
    )
}

async fn models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let planner_health = state.provider_router.health_for_role(&state.planner_model).await;
    let code_health = state.provider_router.health_for_role(&state.code_model).await;
    let math_health = state.provider_router.health_for_role(&state.math_model).await;
    let route_history = state.provider_router.history_snapshot().await;
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "serving_solution": "vllm",
            "protocol": "OpenAI-compatible /v1/chat/completions with /v1/models health and role-specific guided_json schemas",
            "planner": {"config": role_json(&state.planner_model), "health": planner_health},
            "code_generation": {
                "primary": {"config": role_json(&state.code_model), "health": code_health},
                "recommended_options": [
                    "Qwen/Qwen2.5-Coder-7B-Instruct",
                    "Qwen/Qwen2.5-Coder-14B-Instruct",
                    state.code_fallback_model
                ]
            },
            "math_reasoning": {
                "primary": {"config": role_json(&state.math_model), "health": math_health},
                "recommended_options": ["allenai/Olmo-3-7B-Instruct", state.math_large_model]
            },
            "embeddings": {
                "model": env::var("VERITAS_EMBEDDING_MODEL").unwrap_or_else(|_| "Muennighoff/SBERT-base-nli-v2".into()),
                "normalized": true,
                "cosine_search": "OpenSearch FAISS/HNSW"
            },
            "ontology_reasoning": {"graph": "Jena Fuseki SPARQL", "offline_reasoner": "Openllet", "shacl": state.shacl_url.clone()},
            "tool_verified_math_engine": {"url": state.math_tools_url.clone(), "timeout_secs": state.math_tools_timeout_secs, "source_of_truth": "sympy_numpy_scipy_mpmath_hypothesis_service"},
            "provider_router": state.provider_router.summary(),
            "provider_route_history_tail": route_history.iter().rev().take(20).cloned().collect::<Vec<_>>(),
            "remote_fallback": {"enabled": state.remote_model_enabled, "base_url": state.remote_model_base_url, "model": state.remote_model_name, "api_key_env": state.remote_model_api_key_env},
            "execution": {"command_runner": state.command_runner, "sandbox_default": state.command_runner == "docker" || state.command_runner == "sandbox"},
            "shacl": {"url": state.shacl_url, "governance_mode": state.governance_mode.as_str(), "enforced": state.governance_mode.enforces(), "legacy_enforce_env_supported": true}
        })),
    )
}

pub(crate) fn role_json(role: &ModelRole) -> Value {
    json!({
        "role": role.role,
        "url": role.url,
        "huggingface_model_id": role.model,
        "served_model_name": role.served_model_name,
        "temperature": role.temperature,
        "top_p": role.top_p,
        "max_tokens": role.max_tokens,
        "timeout_secs": role.timeout_secs
    })
}

async fn status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let persisted_runs = list_persisted_runs(&state.runs_dir).await.unwrap_or_else(|error| json!({"error": error.to_string()}));
    let run_index_tail = read_events_tail(&state.runs_dir.join("run_index.jsonl"), 100).await.unwrap_or_default();
    let memory_runs = state.recent_runs.lock().await.clone();
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "runs_dir": state.runs_dir.display().to_string(),
            "persistent_runs": persisted_runs,
            "run_index_tail": run_index_tail,
            "recent_runs": memory_runs,
            "message": "Run state is persisted in each run workspace with request.json, state.json, events.jsonl, command_audit.jsonl, run_index.jsonl, artifacts, and final_report.json."
        })),
    )
}


async fn status_by_run_id(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<RunPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    if !workspace.exists() {
        return ApiFailure::new(StatusCode::NOT_FOUND, "run.not_found", format!("Run {} was not found in {}", path.run_id, state.runs_dir.display()), "Check `veritas run list` or verify the configured VERITAS_RUNS_DIR.").response();
    }
    let state_json = read_json_file(&workspace.join("state.json")).await.unwrap_or_else(|| json!({"missing": true}));
    let final_report = read_json_file(&workspace.join("final_report.json")).await;
    let request = read_json_file(&workspace.join("request.json")).await;
    let events = read_events_tail(&workspace.join("events.jsonl"), 50).await.unwrap_or_else(|_| Vec::new());
    let command_audit = read_events_tail(&workspace.join("command_audit.jsonl"), 50).await.unwrap_or_else(|_| Vec::new());
    let human_checkpoints = read_events_tail(&workspace.join("human_checkpoints.jsonl"), 50).await.unwrap_or_else(|_| Vec::new());
    let human_checkpoint_gate = human_checkpoint_gate_summary(&workspace, &state.human_loop_policy).await;
    let lock_metadata = read_json_file(&workspace.join("run.lock")).await;
    let cancelled = workspace.join("CANCELLED").exists();
    let locked = workspace.join("run.lock").exists();
    (StatusCode::OK, Json(json!({
        "ok": true,
        "run_id": path.run_id,
        "workspace": workspace.display().to_string(),
        "cancelled": cancelled,
        "locked": locked,
        "lock_metadata": lock_metadata,
        "request": request,
        "state": state_json,
        "events_tail": events,
        "command_audit_tail": command_audit,
        "human_checkpoints_tail": human_checkpoints,
        "human_checkpoint_gate": human_checkpoint_gate,
        "final_report": final_report
    })))
}

async fn resume_run(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<RunPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    if !workspace.exists() {
        return ApiFailure::new(StatusCode::NOT_FOUND, "run.resume.not_found", format!("Run {} was not found.", path.run_id), "Only persisted run workspaces can be resumed. Start a new run or verify VERITAS_RUNS_DIR.").response();
    }
    if workspace.join("CANCELLED").exists() {
        return ApiFailure::new(StatusCode::CONFLICT, "run.resume.cancelled", format!("Run {} is cancelled.", path.run_id), "Remove the cancellation marker only if you intentionally want to restart this run, or start a new run.").response();
    }
    if workspace.join("final_report.json").exists() {
        let final_report = read_json_file(&workspace.join("final_report.json")).await.unwrap_or_else(|| json!({}));
        return (StatusCode::OK, Json(json!({"ok": true, "run_id": path.run_id, "status": "already_final", "final_report": final_report})));
    }
    match resume_autonomous_run(&state, &path.run_id, workspace).await {
        Ok(report) => (StatusCode::OK, Json(report)),
        Err(error) => error.response(),
    }
}

async fn cancel_run(State(state): State<Arc<AppState>>, AxumPath(path): AxumPath<RunPath>) -> impl IntoResponse {
    let workspace = state.runs_dir.join(&path.run_id);
    if !workspace.exists() {
        return ApiFailure::new(StatusCode::NOT_FOUND, "run.cancel.not_found", format!("Run {} was not found.", path.run_id), "Verify the run id and configured runs directory.").response();
    }
    let cancel_file = workspace.join("CANCELLED");
    match fs::write(&cancel_file, format!("cancelled_at_ms={}\n", now_millis())).await {
        Ok(_) => {
            let _ = persist_run_state(&workspace, "CancelRequested", json!({"run_id": path.run_id, "cancel_file": cancel_file.display().to_string()})).await;
            (StatusCode::OK, Json(json!({"ok": true, "run_id": path.run_id, "status": "cancel_requested", "cancel_file": cancel_file.display().to_string()})))
        }
        Err(error) => ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.cancel.write_failed", format!("Could not write cancellation marker: {error}"), "Check run workspace permissions.").response(),
    }
}

pub(crate) async fn read_json_file(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&text).ok()
}

async fn apply_preloaded_artifacts(workspace: &Path, artifacts: Option<&Value>) -> Result<(), ApiFailure> {
    let Some(artifacts) = artifacts else { return Ok(()); };
    if let Some(report) = artifacts.get("math_validation_report") {
        write_json_file(&workspace.join("math_validation_report.json"), report).await?;
    }
    if let Some(model) = artifacts.get("representation_model") {
        write_json_file(&workspace.join("representation_model.json"), model).await?;
    }
    if let Some(registry) = artifacts.get("evidence_registry") {
        write_json_file(&workspace.join("evidence_registry.json"), registry).await?;
    }
    if let Some(manifest) = artifacts.get("evidence_manifest") {
        write_json_file(&workspace.join("evidence_manifest.json"), manifest).await?;
    }
    if let Some(manifest) = artifacts.get("formula_manifest") {
        write_json_file(&workspace.join("formula_manifest.json"), manifest).await?;
    }
    if let Some(manifest) = artifacts.get("citation_manifest") {
        write_json_file(&workspace.join("citation_manifest.json"), manifest).await?;
    }
    Ok(())
}

pub(crate) async fn read_events_tail(path: &Path, limit: usize) -> Result<Vec<Value>, std::io::Error> {
    let text = match fs::read_to_string(path).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut events: Vec<Value> = text.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok()).collect();
    if events.len() > limit {
        events = events.split_off(events.len() - limit);
    }
    Ok(events)
}

async fn list_persisted_runs(runs_dir: &Path) -> Result<Value, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = match fs::read_dir(runs_dir).await {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!([])),
        Err(error) => return Err(error),
    };
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let run_id = entry.file_name().to_string_lossy().to_string();
        let state_json = read_json_file(&path.join("state.json")).await.unwrap_or_else(|| json!({"missing": true}));
        let final_report = read_json_file(&path.join("final_report.json")).await;
        entries.push(json!({
            "run_id": run_id,
            "workspace": path.display().to_string(),
            "state": state_json.get("state").cloned().unwrap_or_else(|| json!("unknown")),
            "sequence": state_json.get("sequence").cloned().unwrap_or_else(|| json!(null)),
            "cancelled": path.join("CANCELLED").exists(),
            "locked": path.join("run.lock").exists(),
            "final_status": final_report.as_ref().and_then(|v| v.get("final_status")).cloned().unwrap_or_else(|| json!(null)),
            "has_final_report": final_report.is_some()
        }));
    }
    entries.sort_by(|a, b| b.get("run_id").and_then(Value::as_str).cmp(&a.get("run_id").and_then(Value::as_str)));
    Ok(json!(entries))
}

async fn graph_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut warnings: Vec<Value> = Vec::new();
    let counts = json!({
        "objectives": sparql_count(&state, "Objective", &mut warnings).await.unwrap_or(0),
        "plans": sparql_count(&state, "Plan", &mut warnings).await.unwrap_or(0),
        "tasks": sparql_count(&state, "TaskSpecification", &mut warnings).await.unwrap_or(0),
        "risks": sparql_count(&state, "Risk", &mut warnings).await.unwrap_or(0),
        "invariants": sparql_count(&state, "Invariant", &mut warnings).await.unwrap_or(0),
        "evidence_items": sparql_count(&state, "EvidenceArtifact", &mut warnings).await.unwrap_or(0),
        "validation_checks": sparql_count(&state, "ValidationCheckSpecification", &mut warnings).await.unwrap_or(0),
    });
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "counts": counts,
            "ontology": {"name": "Veritas / Invariant Forge OWL-DL", "namespace": "https://github.com/daddydrac/veritas/ontology#"},
            "reasoner": {"name": "Openllet"},
            "graph": {"name": "Fuseki", "query_url": state.fuseki_query_url},
            "vector_memory": {"name": "OpenSearch FAISS/HNSW", "index": state.opensearch_index},
            "warnings": warnings
        })),
    )
}

async fn sparql_count(state: &AppState, class_name: &str, warnings: &mut Vec<Value>) -> Option<u64> {
    let query = format!("PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>\nSELECT (COUNT(?s) AS ?count) WHERE {{ ?s a veritas:{class_name} . }}");
    let response = match state.http.post(&state.fuseki_query_url).header("accept", "application/sparql-results+json").form(&[("query", query)]).send().await {
        Ok(response) => response,
        Err(error) => {
            warnings.push(json!({"stage": "graph_status.transport", "class": class_name, "message": format!("Fuseki count query failed before response: {error}"), "remediation": "Start Fuseki and upload the ontology with `veritas upload-ontology`."}));
            return None;
        }
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        warnings.push(json!({"stage": "graph_status.upstream", "class": class_name, "message": format!("Fuseki count query returned HTTP {status}: {}", text.chars().take(300).collect::<String>()), "remediation": "Verify the Veritas dataset and ontology graph are loaded."}));
        return None;
    }
    let payload: Value = match response.json().await {
        Ok(value) => value,
        Err(error) => {
            warnings.push(json!({"stage": "graph_status.parse", "class": class_name, "message": format!("Fuseki count query response could not be decoded: {error}"), "remediation": "Check Fuseki SPARQL result format."}));
            return None;
        }
    };
    payload.pointer("/results/bindings/0/count/value").and_then(Value::as_str).and_then(|value| value.parse::<u64>().ok())
}

async fn probe(http: &Client, url: &str, service: &str) -> Value {
    match http.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            json!({"service": service, "ok": status.is_success(), "url": url, "http_status": status.as_u16(), "message": if status.is_success() { "reachable" } else { "service responded with non-success HTTP status" }})
        }
        Err(error) => json!({"service": service, "ok": false, "url": url, "error": error.to_string(), "message": "service is unreachable from API container"}),
    }
}

async fn probe_model(http: &Client, role: &ModelRole) -> Value {
    let url = format!("{}/v1/models", role.url.trim_end_matches('/'));
    match http.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            json!({"service": role.role, "ok": status.is_success(), "url": url, "model": role.model, "served_model_name": role.served_model_name, "http_status": status.as_u16(), "message": if status.is_success() { "vLLM OpenAI-compatible model endpoint reachable" } else { "vLLM responded with non-success HTTP status" }})
        }
        Err(error) => json!({"service": role.role, "ok": false, "url": url, "model": role.model, "served_model_name": role.served_model_name, "error": error.to_string(), "message": "vLLM endpoint is unavailable; start model profiles or configure a remote fallback"}),
    }
}

async fn sparql(State(state): State<Arc<AppState>>, Json(req): Json<SparqlRequest>) -> impl IntoResponse {
    match run_sparql(&state, &req.query).await {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "operation": "sparql.query", "service": "fuseki", "result": value}))),
        Err(error) => error.response(),
    }
}

async fn search(State(state): State<Arc<AppState>>, Json(req): Json<SearchRequest>) -> impl IntoResponse {
    match retrieve_evidence(&state, &req.query, req.size.unwrap_or(10), req.mode.as_deref()).await {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "operation": "search.query", "service": "opensearch", "result": value}))),
        Err(error) => error.response(),
    }
}

async fn llm_chat(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> impl IntoResponse {
    let role = input.get("role").and_then(Value::as_str).unwrap_or("planner");
    let prompt = match input.get("prompt").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => value,
        None => return ApiFailure::new(StatusCode::BAD_REQUEST, "llm.validation", "Missing non-empty `prompt`.", "Pass a prompt and optional role: planner, code, or math.").response(),
    };
    let model = match role {
        "code" | "code_generation" => &state.code_model,
        "math" | "math_reasoning" => &state.math_model,
        _ => &state.planner_model,
    };
    match call_chat_model_text(&state, model, "You are Veritas. Return precise, evidence-backed, implementation-oriented output.", prompt).await {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "role": role, "model": role_json(model), "result": value}))),
        Err(value) => value.response(),
    }
}

async fn plan(State(state): State<Arc<AppState>>, Json(input): Json<Value>) -> impl IntoResponse {
    let goal = match extract_goal(&input) {
        Ok(goal) => goal,
        Err(error) => return error.response(),
    };
    let size = input.get("size").and_then(Value::as_u64).unwrap_or(8).min(50) as u32;
    let workspace_path = input.get("workspace")
        .or_else(|| input.get("workspace_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    match build_structured_plan(&state, &goal, size, workspace_path.as_deref(), Some(&input)).await {
        Ok(plan) => (StatusCode::OK, Json(plan)),
        Err(error) => error.response(),
    }
}

async fn run_agent(State(state): State<Arc<AppState>>, Json(req): Json<RunRequest>) -> impl IntoResponse {
    match execute_autonomous_run(&state, req).await {
        Ok(report) => {
            let mut runs = state.recent_runs.lock().await;
            runs.push(report.clone());
            if runs.len() > 20 {
                let drain_count = runs.len() - 20;
                runs.drain(0..drain_count);
            }
            (StatusCode::OK, Json(report))
        }
        Err(error) => error.response(),
    }
}

fn extract_goal(input: &Value) -> Result<String, ApiFailure> {
    input
        .get("goal")
        .or_else(|| input.get("prompt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| ApiFailure::new(StatusCode::BAD_REQUEST, "plan.validation", "Request is missing a non-empty `goal` or `prompt`.", "Ask Veritas what you want built, analyzed, or converted from research into code."))
}



async fn opensearch_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let versioned = probe_opensearch_head(&state, &state.opensearch_versioned_index).await;
    let read_alias = probe_opensearch_head(&state, &state.opensearch_read_alias).await;
    let write_alias = probe_opensearch_head(&state, &state.opensearch_write_alias).await;
    let legacy_index = probe_opensearch_head(&state, &state.opensearch_base_index).await;
    let aliases = fetch_opensearch_aliases(&state).await.unwrap_or_else(|error| json!({"ok": false, "error": error.message, "remediation": error.remediation}));
    (StatusCode::OK, Json(json!({
        "ok": true,
        "base_index": state.opensearch_base_index,
        "versioned_index": state.opensearch_versioned_index,
        "mapping_version": state.opensearch_mapping_version,
        "read_alias": state.opensearch_read_alias,
        "write_alias": state.opensearch_write_alias,
        "active_search_target": state.opensearch_index,
        "vector_field": state.opensearch_vector_field,
        "vector_dimension": state.opensearch_vector_dimension,
        "checks": {"versioned_index": versioned, "read_alias": read_alias, "write_alias": write_alias, "legacy_index": legacy_index},
        "aliases": aliases,
        "business_outcome": "OpenSearch is treated as a versioned FAISS/HNSW evidence memory with stable read/write aliases so schema migrations do not corrupt retrieval."
    })))
}

async fn opensearch_mapping(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({
        "ok": true,
        "base_index": state.opensearch_base_index,
        "versioned_index": state.opensearch_versioned_index,
        "read_alias": state.opensearch_read_alias,
        "write_alias": state.opensearch_write_alias,
        "mapping_version": state.opensearch_mapping_version,
        "mapping": production_opensearch_mapping(&state.opensearch_vector_field, state.opensearch_vector_dimension, &state.opensearch_mapping_version)
    })))
}

async fn opensearch_migrate(State(state): State<Arc<AppState>>, request: Option<Json<OpenSearchMigrateRequest>>) -> impl IntoResponse {
    let req = request.map(|Json(v)| v).unwrap_or(OpenSearchMigrateRequest { dry_run: None, force_alias_update: None, version: None });
    let version = req.version.as_deref().unwrap_or(&state.opensearch_mapping_version);
    let target_index = if version == state.opensearch_mapping_version {
        state.opensearch_versioned_index.clone()
    } else {
        format!("{}-{}", state.opensearch_base_index, version)
    };
    let mapping = production_opensearch_mapping(&state.opensearch_vector_field, state.opensearch_vector_dimension, version);
    let force_alias_update = req.force_alias_update.unwrap_or(true);
    let migration_plan = json!({
        "target_index": target_index,
        "base_index": state.opensearch_base_index,
        "read_alias": state.opensearch_read_alias,
        "write_alias": state.opensearch_write_alias,
        "mapping_version": version,
        "force_alias_update": force_alias_update,
        "vector_field": state.opensearch_vector_field,
        "vector_dimension": state.opensearch_vector_dimension,
        "actions": [
            "create versioned index if absent",
            "attach read alias to versioned index",
            "attach write alias to versioned index with is_write_index=true",
            "preserve existing indices; do not delete data during migration"
        ]
    });
    if req.dry_run.unwrap_or(false) {
        return (StatusCode::OK, Json(json!({"ok": true, "dry_run": true, "plan": migration_plan, "mapping": mapping})));
    }

    let target_url = format!("{}/{}", state.opensearch_url.trim_end_matches('/'), target_index);
    let exists = match state.http.head(&target_url).send().await {
        Ok(response) => response.status().is_success(),
        Err(error) => return ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.migrate.transport", format!("OpenSearch HEAD failed: {error}"), "Start OpenSearch and check VERITAS_OPENSEARCH_URL.").response(),
    };
    let mut steps = Vec::new();
    if !exists {
        let response = match state.http.put(&target_url).json(&mapping).send().await {
            Ok(response) => response,
            Err(error) => return ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.migrate.put_failed", format!("OpenSearch PUT failed: {error}"), "Check OpenSearch health and mapping JSON.").response(),
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
        if !status.is_success() {
            return ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.migrate.upstream", format!("OpenSearch returned HTTP {} while creating index", status.as_u16()), "Inspect OpenSearch logs and the generated mapping.").with_details(body).response();
        }
        steps.push(json!({"stage": "create_index", "index": target_index, "response": body}));
    } else {
        steps.push(json!({"stage": "create_index", "index": target_index, "status": "already_exists"}));
    }

    let alias_actions = build_alias_actions(&state, &target_index, force_alias_update).await;
    if !alias_actions.is_empty() {
        let alias_url = format!("{}/_aliases", state.opensearch_url.trim_end_matches('/'));
        let response = match state.http.post(&alias_url).json(&json!({"actions": alias_actions})).send().await {
            Ok(response) => response,
            Err(error) => return ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.alias.transport", format!("OpenSearch alias update failed before response: {error}"), "Check OpenSearch health and alias settings.").response(),
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
        if !status.is_success() {
            return ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.alias.upstream", format!("OpenSearch returned HTTP {} while updating aliases", status.as_u16()), "Inspect alias conflicts and rerun with a clean versioned index.").with_details(body).response();
        }
        steps.push(json!({"stage": "alias_update", "response": body}));
    }

    (StatusCode::OK, Json(json!({
        "ok": true,
        "status": "migrated",
        "plan": migration_plan,
        "steps": steps,
        "message": "OpenSearch FAISS/HNSW mapping is versioned and attached to stable read/write aliases."
    })))
}


async fn probe_opensearch_head(state: &AppState, target: &str) -> Value {
    let url = format!("{}/{}", state.opensearch_url.trim_end_matches('/'), target);
    match state.http.head(&url).send().await {
        Ok(response) => json!({"target": target, "ok": response.status().is_success(), "http_status": response.status().as_u16()}),
        Err(error) => json!({"target": target, "ok": false, "error": error.to_string()})
    }
}

async fn fetch_opensearch_aliases(state: &AppState) -> Result<Value, ApiFailure> {
    let url = format!("{}/_aliases", state.opensearch_url.trim_end_matches('/'));
    let response = state.http.get(&url).send().await
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.aliases.transport", format!("OpenSearch alias lookup failed: {error}"), "Start OpenSearch and check the configured URL."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "opensearch.aliases.upstream", format!("OpenSearch returned HTTP {} while listing aliases", status.as_u16()), "Inspect OpenSearch logs.").with_details(body));
    }
    Ok(body)
}

async fn build_alias_actions(state: &AppState, target_index: &str, force_alias_update: bool) -> Vec<Value> {
    let mut actions = Vec::new();
    if force_alias_update {
        if let Ok(aliases) = fetch_opensearch_aliases(state).await {
            if let Some(indices) = aliases.as_object() {
                for (index_name, index_body) in indices {
                    let alias_map = index_body.get("aliases").and_then(Value::as_object);
                    if let Some(alias_map) = alias_map {
                        for alias in [&state.opensearch_read_alias, &state.opensearch_write_alias] {
                            if alias_map.contains_key(alias) {
                                actions.push(json!({"remove": {"index": index_name, "alias": alias}}));
                            }
                        }
                    }
                }
            }
        }
    }
    actions.push(json!({"add": {"index": target_index, "alias": state.opensearch_read_alias}}));
    actions.push(json!({"add": {"index": target_index, "alias": state.opensearch_write_alias, "is_write_index": true}}));
    actions
}

async fn graphs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let query = "SELECT DISTINCT ?graph WHERE { GRAPH ?graph { ?s ?p ?o } } ORDER BY ?graph LIMIT 500";
    let result = run_sparql(&state, query).await.unwrap_or_else(|error| json!({"ok": false, "error": {"code": error.code, "message": error.message, "remediation": error.remediation}}));
    (StatusCode::OK, Json(json!({
        "ok": true,
        "configured_graphs": {
            "ontology": state.graph_ontology_uri,
            "document_base": state.graph_document_base_uri,
            "run_base": state.graph_run_base_uri,
            "validation_base": state.graph_validation_base_uri
        },
        "named_graph_query": result,
        "note": "Fuseki stores ontology TBox facts plus project ABox facts. PDF binaries remain in file/object storage; RDF stores semantic links."
    })))
}

async fn graph_describe(State(state): State<Arc<AppState>>, Json(req): Json<GraphDescribeRequest>) -> impl IntoResponse {
    if req.graph_uri.trim().is_empty() {
        return ApiFailure::new(StatusCode::BAD_REQUEST, "graph.describe.validation", "graph_uri is required.", "Pass the named graph URI to inspect.").response();
    }
    let query = format!("SELECT (COUNT(*) AS ?triples) WHERE {{ GRAPH <{}> {{ ?s ?p ?o }} }}", req.graph_uri);
    match run_sparql(&state, &query).await {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "graph_uri": req.graph_uri, "result": value}))),
        Err(error) => error.response(),
    }
}

async fn graph_upload(State(state): State<Arc<AppState>>, Json(req): Json<GraphUploadRequest>) -> impl IntoResponse {
    let graph_uri = req.graph_uri.unwrap_or_else(|| state.graph_ontology_uri.clone());
    if graph_uri.trim().is_empty() || req.turtle.trim().is_empty() {
        return ApiFailure::new(StatusCode::BAD_REQUEST, "graph.upload.validation", "graph_uri and non-empty turtle are required.", "Provide Turtle RDF and a named graph URI.").response();
    }
    match upload_turtle_to_fuseki(state.as_ref(), &graph_uri, &req.turtle, req.replace.unwrap_or(false), req.content_type.as_deref().unwrap_or("text/turtle")).await {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "graph_uri": graph_uri, "upload": value}))),
        Err(error) => error.response(),
    }
}

async fn graph_facts(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let facts = planner_fact_summary(&state).await;
    (StatusCode::OK, Json(json!({"ok": true, "planner_fact_summary": facts})))
}

async fn upload_turtle_to_fuseki(state: &AppState, graph_uri: &str, turtle: &str, replace: bool, content_type: &str) -> Result<Value, ApiFailure> {
    let method = if replace { "PUT" } else { "POST" };
    let request = if replace { state.http.put(&state.fuseki_data_url) } else { state.http.post(&state.fuseki_data_url) };
    let response = request
        .query(&[("graph", graph_uri)])
        .header("content-type", content_type)
        .body(turtle.to_string())
        .send()
        .await
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "fuseki.upload.transport", format!("Fuseki graph upload failed before response: {error}"), "Start Fuseki and check VERITAS_FUSEKI_DATA_URL."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "fuseki.upload.upstream", format!("Fuseki returned HTTP {} while uploading graph", status.as_u16()), "Verify dataset, graph-store endpoint, content type, and RDF syntax.").with_details(body));
    }
    Ok(json!({"method": method, "graph_uri": graph_uri, "http_status": status.as_u16()}))
}

async fn planner_fact_summary(state: &AppState) -> Value {
    let mut summaries = serde_json::Map::new();
    let mut warnings = Vec::new();
    for (name, query) in query_pack() {
        match run_sparql(state, query).await {
            Ok(value) => {
                let bindings = value.pointer("/results/bindings").and_then(Value::as_array).cloned().unwrap_or_default();
                let samples: Vec<Value> = bindings.iter().take(5).map(flatten_sparql_binding).collect();
                summaries.insert(name.to_string(), json!({"ok": true, "count": bindings.len(), "samples": samples}));
            }
            Err(error) => {
                warnings.push(json!({"query": name, "code": error.code, "message": error.message, "remediation": error.remediation}));
                summaries.insert(name.to_string(), json!({"ok": false, "count": 0}));
            }
        }
    }
    json!({"queries": summaries, "warnings": warnings})
}

fn query_pack() -> Vec<(&'static str, &'static str)> {
    vec![
        ("formula_traceability", include_str!("../../../packages/ontology/queries/formula_traceability.sparql")),
        ("evidence_chunks", include_str!("../../../packages/ontology/queries/evidence_chunks.sparql")),
        ("formulas_without_invariants", include_str!("../../../packages/ontology/queries/formulas_without_invariants.sparql")),
        ("risks_without_mitigation", include_str!("../../../packages/ontology/queries/risks_without_mitigation.sparql")),
        ("plans_without_validation", include_str!("../../../packages/ontology/queries/plans_without_validation.sparql")),
        ("unvalidated_code_artifacts", include_str!("../../../packages/ontology/queries/unvalidated_code_artifacts.sparql")),
        ("builds_without_tests", include_str!("../../../packages/ontology/queries/builds_without_tests.sparql")),
        ("loops_without_termination", include_str!("../../../packages/ontology/queries/loops_without_termination.sparql")),
        ("objectives_blocked_by_assumptions", include_str!("../../../packages/ontology/queries/objectives_blocked_by_assumptions.sparql")),
        ("deployment_units_without_observability", include_str!("../../../packages/ontology/queries/deployment_units_without_observability.sparql")),
        ("math_claims_without_transfer_tests", include_str!("../../../packages/ontology/queries/math_claims_without_transfer_tests.sparql")),
    ]
}

fn flatten_sparql_binding(binding: &Value) -> Value {
    let mut flat = serde_json::Map::new();
    if let Some(obj) = binding.as_object() {
        for (key, value) in obj {
            flat.insert(key.clone(), value.get("value").cloned().unwrap_or_else(|| value.clone()));
        }
    }
    Value::Object(flat)
}

async fn evidence_registry_status(Json(req): Json<EvidenceRegistryStatusRequest>) -> impl IntoResponse {
    (StatusCode::OK, Json(evidence_registry::registry_status_payload(req.path.as_deref()).await))
}

async fn math_to_code(State(state): State<Arc<AppState>>, Json(req): Json<MathToCodeRequest>) -> impl IntoResponse {
    let language = req.language.clone().unwrap_or_else(|| "rust".to_string());
    let eligibility = match evidence_registry::require_math_to_code_eligibility(&req).await {
        Ok(value) => value,
        Err(error) => return error.response(),
    };
    let formula_context = json!({
        "formula_id": eligibility.formula.get("formula_id").cloned().unwrap_or_else(|| json!(req.formula_id)),
        "formula_record_id": req.formula_record_id,
        "citation_record_id": req.citation_record_id,
        "evidence_manifest_path": req.evidence_manifest_path,
        "evidence_registry_path": eligibility.registry_path.as_ref().map(|path| path.display().to_string()),
        "formula_latex": eligibility.formula.get("normalized_latex").or_else(|| eligibility.formula.get("raw_latex")).cloned().unwrap_or_else(|| json!(req.formula_latex)),
        "prompt": req.prompt,
        "evidence_eligibility": eligibility.formula_context,
        "artifact_status_floor": eligibility.status_floor(),
        "required_method": "surface phenomenon -> representation map -> latent ontology -> transformation space -> invariant -> compression fidelity -> recursive closure -> generative necessity -> symbolic shadow -> transfer -> validation -> code"
    });
    let math_prompt = build_math_to_code_reasoning_prompt(&formula_context, &language);
    let math_reasoning = match call_chat_model_json(&state, &state.math_model, SchemaKey::MathReasoning, math_to_code_system_prompt(), &math_prompt).await {
        Ok(value) => value,
        Err(error) => return error.response(),
    };
    let tool_verified_math = match math_tools::validate_formula_context(&state, &formula_context).await {
        Ok(value) => value,
        Err(error) => return error.response(),
    };
    if !tool_verified_math.get("ok").and_then(Value::as_bool).unwrap_or(false) && !eligibility.exploratory_unverified {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "ok": false,
                "state": "blocked_by_math_tools",
                "status": "blocked_by_math_tools",
                "message": "Tool-Verified Math Engine rejected the formula before code generation.",
                "math_validation_report": tool_verified_math,
                "files_written": [],
                "commands_run": [],
                "next_action": "Fix the formula, assumptions, or evidence registry before retrying formula-to-code."
            })),
        );
    }
    let checkpoint = math_human_checkpoint(&state, &math_reasoning, &formula_context, req.human_decision.clone());
    if let Err(error) = validate_model_json(SchemaKey::HumanCheckpoint, &checkpoint) {
        return error.response();
    }
    if checkpoint.get("required").and_then(Value::as_bool).unwrap_or(false) && !checkpoint.get("approved").and_then(Value::as_bool).unwrap_or(false) {
        return (
            StatusCode::ACCEPTED,
            Json(json!({
                "ok": false,
                "status": "awaiting_human_checkpoint",
                "message": "Veritas completed registry-authorized representation-first math analysis and needs human approval before code generation.",
                "checkpoint": checkpoint,
                "math_reasoning": math_reasoning,
                "evidence_eligibility": formula_context.get("evidence_eligibility").cloned().unwrap_or_else(|| json!({})),
                "next_action": "Approve the representation review with human_decision={\"decision\":\"approve\"}; formula/citation approval must remain persisted in evidence_registry.json."
            })),
        );
    }
    let goal = format!(
        "Math-to-code task. Treat the formula/problem as a symbolic shadow, not truth. Use this registry-authorized representation-first math analysis to generate tested {language} code. Formula context: {}. Math reasoning JSON: {}. Human checkpoint: {}",
        formula_context,
        math_reasoning,
        checkpoint,
    );
    let run_req = RunRequest {
        goal,
        language: Some(language),
        size: Some(8),
        max_retries: req.max_retries,
        execution_mode: Some("production".to_string()),
        preloaded_artifacts: Some(json!({
            "math_validation_report": tool_verified_math,
            "representation_model": {
                "kind": "VeritasRepresentationModel",
                "status": "approved",
                "surface_phenomenon": math_reasoning.get("surface_phenomenon").cloned().unwrap_or_else(|| json!("formula-to-code request")),
                "representation_map": math_reasoning.get("candidate_representation_map").or_else(|| math_reasoning.get("representation_map")).cloned().unwrap_or_else(|| json!({"type":"formula_context_to_implementation_spec"})),
                "invariants": math_reasoning.get("invariants").cloned().unwrap_or_else(|| json!([])),
                "symbolic_shadows": math_reasoning.get("symbolic_shadows").cloned().unwrap_or_else(|| json!([formula_context.get("formula_latex").cloned().unwrap_or_else(|| json!(""))])),
                "validation_obligations": math_reasoning.get("validation_requirements").or_else(|| math_reasoning.get("validation_obligations")).cloned().unwrap_or_else(|| json!(["tool_verified_math_engine", "compile_and_test"])),
                "tool_verified_math_report_path": "math_validation_report.json"
            })
        })),
    };
    match execute_autonomous_run(&state, run_req).await {
        Ok(mut report) => {
            report["kind"] = json!("VeritasMathToCodeRunReport");
            report["math_reasoning"] = math_reasoning;
            report["tool_verified_math"] = tool_verified_math;
            report["human_checkpoint"] = checkpoint;
            report["evidence_eligibility"] = formula_context.get("evidence_eligibility").cloned().unwrap_or_else(|| json!({}));
            if eligibility.exploratory_unverified {
                report["ok"] = json!(false);
                report["final_status"] = json!("generated_unvalidated");
                report["generated_package_status"] = json!("generated_unvalidated");
                report["production_status_allowed"] = json!(false);
                report["remaining_limitations"] = json!(["Formula was supplied without an approved Evidence Eligibility Registry; artifact is exploratory and cannot be production-bound."]);
            }
            (StatusCode::OK, Json(report))
        }
        Err(error) => error.response(),
    }
}

fn math_to_code_system_prompt() -> &'static str {
    "You are Veritas Mathematical Researcher. Return valid JSON only. Follow MATH.md exactly: do not treat equations as truth; treat formulas as symbolic shadows of deeper transformational structure. Analyze surface phenomenon, representation hypothesis, representation map, primitive ontology, transformations, constraints, invariants, compression fidelity, recursion, generative necessity, symbolic shadows, transfer, risks, and validation requirements. Do not expose private chain-of-thought; provide concise auditable reasoning summaries."
}

fn build_math_to_code_reasoning_prompt(formula_context: &Value, language: &str) -> String {
    json!({
        "task": "representation_first_math_to_code_analysis",
        "language": language,
        "formula_context": formula_context,
        "required_json_schema": schema_description_for(SchemaKey::MathReasoning),
        "math_discipline": {
            "central_warning": "surface form != deep structure",
            "formula_role": "SymbolicShadow",
            "default_path": ["surface_phenomenon", "representation_search", "latent_ontology", "transformation_space", "constraint_geometry", "invariants", "compression_fidelity", "recursive_closure", "generative_necessity", "symbolic_shadows", "transfer_tests", "validation_requirements"],
            "epistemic_gates": ["observation_not_truth", "equation_not_origin", "pattern_not_invariant", "simulation_not_truth", "prediction_not_understanding", "compression_not_truth", "training_fit_not_transfer"]
        },
        "hard_requirements": [
            "Return JSON only.",
            "Include Axiom Map A0-A15 references where relevant.",
            "Every invariant must specify its transformation family.",
            "Every symbolic shadow must state generative origin, scope, and failure conditions.",
            "Every implementation note must produce validation requirements."
        ]
    }).to_string()
}

fn math_human_checkpoint(state: &AppState, math_reasoning: &Value, formula_context: &Value, human_decision: Option<Value>) -> Value {
    let policy = state.human_loop_policy.as_str();
    let high_risk = math_reasoning.get("risks").and_then(Value::as_array).map(|items| items.iter().any(|risk| {
        let text = risk.to_string().to_ascii_lowercase();
        text.contains("high") || text.contains("critical") || text.contains("under-specified") || text.contains("unspecified") || text.contains("unproved")
    })).unwrap_or(true);
    let required = match policy {
        "auto_approve" => false,
        "require_all" => true,
        "require_high_risk_only" => high_risk,
        _ => high_risk,
    };
    let decision_value = human_decision.unwrap_or_else(|| if required { json!({"decision":"pending"}) } else { json!({"decision":"auto_approve", "reason":"policy_allows_non_high_risk_auto_approval"}) });
    let decision = decision_value.get("decision").and_then(Value::as_str).unwrap_or("pending").to_ascii_lowercase();
    let approved = matches!(decision.as_str(), "approve" | "edit" | "auto_approve") || !required;
    json!({
        "phase": "math_to_code_representation_review",
        "policy": policy,
        "required": required,
        "approved": approved,
        "decision": decision_value,
        "question": "Do you approve this formula as a symbolic shadow with the stated representation map, invariants, risks, and validation requirements for code generation? Formula/citation approval is read only from the Evidence Eligibility Registry.",
        "artifact": {"formula_context": formula_context, "math_reasoning": math_reasoning},
        "options": ["approve", "edit", "reject", "ask_for_explanation"],
        "status": if approved { "approved_or_auto_approved" } else { "pending_human_review" }
    })
}




async fn math_tools_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match math_tools::status(&state).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.response(),
    }
}

async fn math_tools_validate(State(state): State<Arc<AppState>>, Json(req): Json<MathToolsValidateRequest>) -> impl IntoResponse {
    if let Some(workspace) = req.workspace.as_ref() {
        let path = PathBuf::from(workspace);
        let goal = req.goal.clone().unwrap_or_else(|| "math validation".to_string());
        let plan = read_json_file(&path.join("plan.json")).await.unwrap_or_else(|| json!({}));
        return match math_tools::validate_workspace_if_required(&state, &path, &goal, &plan).await {
            Ok(Some(value)) => Json(value).into_response(),
            Ok(None) => Json(json!({"ok": true, "status": "not_applicable_non_math_run", "workspace": workspace})).into_response(),
            Err(error) => error.response(),
        };
    }
    let formula_context = json!({
        "formula_id": req.formula_id,
        "formula_latex": req.formula_latex,
        "assumptions": req.assumptions.unwrap_or_default(),
        "variables": req.variables.unwrap_or_default(),
    });
    match math_tools::validate_formula_context(&state, &formula_context).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.response(),
    }
}

async fn human_checkpoint(State(state): State<Arc<AppState>>, Json(req): Json<HumanCheckpointRequest>) -> impl IntoResponse {
    let decision = req.decision.trim().to_ascii_lowercase();
    if !["pending", "approve", "edit", "reject", "skip", "auto_approve", "ask_for_explanation"].contains(&decision.as_str()) {
        return ApiFailure::new(StatusCode::BAD_REQUEST, "human_checkpoint.invalid_decision", format!("Unsupported decision: {}", req.decision), "Use pending, approve, edit, reject, skip, auto_approve, or ask_for_explanation.").response();
    }
    let phase = req.phase.trim().to_ascii_lowercase();
    let allowed_phases = ["citation_review", "formula_review", "representation_review", "plan_review", "code_architecture_review", "validation_review", "math_to_code_representation_review"];
    if !allowed_phases.contains(&phase.as_str()) {
        return ApiFailure::new(StatusCode::BAD_REQUEST, "human_checkpoint.invalid_phase", format!("Unsupported checkpoint phase: {}", req.phase), "Use citation_review, formula_review, representation_review, plan_review, code_architecture_review, validation_review, or math_to_code_representation_review.").response();
    }
    let run_id = req.run_id.clone().unwrap_or_else(|| format!("ad_hoc-{}", now_millis()));
    let workspace = state.runs_dir.join(&run_id);
    if let Err(error) = fs::create_dir_all(&workspace).await {
        return ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "human_checkpoint.workspace", format!("Could not create checkpoint workspace: {error}"), "Check VERITAS_RUNS_DIR permissions.").response();
    }
    let policy = req.policy.unwrap_or_else(|| state.human_loop_policy.clone());
    let notes = req.notes.unwrap_or_default();
    let artifact_value = req.artifact.clone().unwrap_or_else(|| json!({}));
    let required = req.required.unwrap_or_else(|| human_checkpoint_required(&policy, &phase, &artifact_value));
    let approved = human_decision_approved(&decision, &notes) || (!required && decision != "reject");
    let blocked = human_decision_blocks(&decision, required, &notes);
    let checkpoint = json!({
        "kind": "HumanCheckpoint",
        "run_id": run_id.clone(),
        "phase": phase.clone(),
        "policy": policy.clone(),
        "required": required,
        "decision": decision.clone(),
        "approved": approved,
        "blocked": blocked,
        "waived": decision == "skip" && !notes.trim().is_empty(),
        "reviewer": req.reviewer.unwrap_or_else(|| "human".to_string()),
        "notes": notes.clone(),
        "artifact": artifact_value,
        "timestamp_ms": now_millis(),
        "status": if approved { "approved_or_waived" } else if blocked { "blocked" } else { "recorded" }
    });
    if let Err(error) = append_jsonl(&workspace.join("human_checkpoints.jsonl"), &checkpoint).await {
        return ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "human_checkpoint.persist", format!("Could not persist checkpoint: {error}"), "Check run workspace permissions.").response();
    }
    let _ = persist_run_state(&workspace, "HumanCheckpointRecorded", checkpoint.clone()).await;
    let gate = human_checkpoint_gate_summary(&workspace, &state.human_loop_policy).await;
    (StatusCode::OK, Json(json!({"ok": true, "checkpoint": checkpoint, "human_checkpoint_gate": gate, "workspace": workspace.display().to_string()})))
}

pub(crate) fn human_decision_approved(decision: &str, notes: &str) -> bool {
    matches!(decision, "approve" | "edit" | "auto_approve") || (decision == "skip" && !notes.trim().is_empty())
}

pub(crate) fn human_decision_blocks(decision: &str, required: bool, notes: &str) -> bool {
    if decision == "reject" { return true; }
    if !required { return false; }
    !human_decision_approved(decision, notes)
}

pub(crate) fn human_checkpoint_required(policy: &str, phase: &str, artifact: &Value) -> bool {
    match policy {
        "auto_approve" => false,
        "require_all" => true,
        "require_high_risk_only" => {
            matches!(phase, "representation_review" | "plan_review" | "code_architecture_review" | "validation_review" | "math_to_code_representation_review")
                || artifact.to_string().to_ascii_lowercase().contains("high")
                || artifact.to_string().to_ascii_lowercase().contains("critical")
                || artifact.to_string().to_ascii_lowercase().contains("failed")
                || artifact.to_string().to_ascii_lowercase().contains("unproved")
        }
        _ => true,
    }
}

pub(crate) async fn human_checkpoint_gate_summary(workspace: &Path, policy: &str) -> Value {
    let checkpoints = read_events_tail(&workspace.join("human_checkpoints.jsonl"), 500).await.unwrap_or_default();
    let required_phases = ["citation_review", "formula_review", "representation_review", "plan_review", "code_architecture_review", "validation_review"];
    let mut latest: HashMap<String, Value> = HashMap::new();
    for checkpoint in checkpoints.iter() {
        if let Some(phase) = checkpoint.get("phase").and_then(Value::as_str) {
            latest.insert(phase.to_string(), checkpoint.clone());
        }
    }
    let mut missing = Vec::new();
    let mut blocked = Vec::new();
    let mut approved = Vec::new();
    let mut waived = Vec::new();
    let mut rejected = Vec::new();
    for phase in required_phases {
        match latest.get(phase) {
            Some(checkpoint) => {
                if checkpoint.get("waived").and_then(Value::as_bool).unwrap_or(false) { waived.push(phase); }
                if checkpoint.get("approved").and_then(Value::as_bool).unwrap_or(false) { approved.push(phase); }
                if checkpoint.get("decision").and_then(Value::as_str) == Some("reject") { rejected.push(phase); blocked.push(phase); }
                else if checkpoint.get("blocked").and_then(Value::as_bool).unwrap_or(false) { blocked.push(phase); }
            }
            None => {
                if policy == "require_all" { missing.push(phase); blocked.push(phase); }
            }
        }
    }
    json!({
        "policy": policy,
        "checkpoint_count": checkpoints.len(),
        "approved_phases": approved,
        "waived_phases": waived,
        "missing_phases": missing,
        "rejected_phases": rejected,
        "blocked_phases": blocked,
        "can_continue": blocked.is_empty(),
        "production_status_allowed": blocked.is_empty(),
        "note": "Phase 7 human workflow gate covers citation, formula, representation, plan, code architecture, and validation review."
    })
}

fn production_opensearch_mapping(vector_field: &str, dimension: usize, version: &str) -> Value {
    json!({
        "settings": {"index": {"number_of_shards": 1, "number_of_replicas": 0, "knn": true, "knn.algo_param.ef_search": 100}},
        "mappings": {"_meta": {"schema": "veritas_evidence", "version": version, "vector_field": vector_field, "vector_dimension": dimension, "owner": "veritas-rust-api"}, "properties": {
            "doc_id": {"type": "keyword"},
            "paper_id": {"type": "keyword"},
            "chunk_id": {"type": "keyword"},
            "formula_id": {"type": "keyword"},
            "run_id": {"type": "keyword"},
            "source_type": {"type": "keyword"},
            "status": {"type": "keyword"},
            "sha256": {"type": "keyword"},
            "title": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 512}}},
            "abstract": {"type": "text"},
            "apa_citation": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 1024}}},
            "text": {"type": "text"},
            "chunk_text": {"type": "text"},
            "formula_description": {"type": "text"},
            "technical_summary": {"type": "text"},
            "embedding_model": {"type": "keyword"},
            "embedding_norm": {"type": "float"},
            vector_field: {"type": "knn_vector", "dimension": dimension, "space_type": "cosinesimil", "method": {"name": "hnsw", "engine": "faiss", "parameters": {"ef_construction": 128, "m": 24}}},
            "formula_embedding": {"type": "knn_vector", "dimension": dimension, "space_type": "cosinesimil", "method": {"name": "hnsw", "engine": "faiss", "parameters": {"ef_construction": 128, "m": 24}}},
            "formulas": {"type": "nested", "properties": {
                "formula_id": {"type": "keyword"},
                "latex": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 4096}}},
                "normalized_latex": {"type": "text", "fields": {"raw": {"type": "keyword", "ignore_above": 4096}}},
                "description": {"type": "text"},
                "formula_image_path": {"type": "keyword"},
                "formula_image_status": {"type": "keyword"},
                "latex_ocr_status": {"type": "keyword"},
                "page": {"type": "integer"},
                "bbox": {"type": "float"},
                "source": {"type": "keyword"},
                "pattern": {"type": "keyword"},
                "confidence": {"type": "float"},
                "human_validated": {"type": "boolean"}
            }},
            "citations": {"type": "nested", "properties": {"apa": {"type": "text"}, "doi": {"type": "keyword"}, "url": {"type": "keyword"}, "validated": {"type": "boolean"}}},
            "metadata": {"type": "object", "enabled": true}
        }}
    })
}

async fn build_structured_plan(state: &AppState, goal: &str, size: u32, workspace: Option<&Path>, request_context: Option<&Value>) -> Result<Value, ApiFailure> {
    let search_attempt = retrieve_evidence(state, goal, size, Some("hybrid")).await;
    let (raw_evidence, opensearch_error) = match search_attempt {
        Ok(value) => (value, None),
        Err(error) => {
            let details = json!({"code": error.code, "message": error.message, "remediation": error.remediation, "details": error.details});
            (json!({"hits": {"hits": []}, "veritas_search_error": details.clone()}), Some(details))
        }
    };
    let evidence = compact_search_evidence(&raw_evidence);
    let formula_trace = run_formula_trace_query(state).await.unwrap_or_else(|error| {
        json!({"ok": false, "warning": {"code": error.code, "message": error.message, "remediation": error.remediation}})
    });
    let ontology_facts = planner_fact_summary(state).await;
    let planning_context = planning_context::build(planning_context::PlanningContextInput {
        workspace,
        goal,
        size,
        opensearch_evidence: &evidence,
        opensearch_error,
        formula_trace: &formula_trace,
        ontology_facts: &ontology_facts,
        request_context,
    }).await?;
    planning_context::write_context(workspace, &planning_context).await?;
    let system = r#"You are the Veritas autonomous planner. You must return valid JSON only. No markdown. No prose outside JSON. Produce an evidence-backed plan using only the approved ids in planning_context. Follow representation-first mathematical research: surface phenomenon, symbolic shadow, invariant, risk, plan, tasks, validation, build artifact. Do not claim production readiness until compile/test validation passes."#;
    let user = json!({
        "goal": goal,
        "required_json_schema": schema_description_for(SchemaKey::Planner),
        "available_tools": ["retrieval", "sparql", "shacl_validate", "math_reasoning", "code_generation", "local_command", "test_runner"],
        "planning_context": planning_context,
        "planner_lineage_contract": planning_context::planner_prompt_contract(&planning_context),
        "opensearch_evidence": evidence,
        "sparql_formula_trace": formula_trace,
        "ontology_planner_fact_summary": ontology_facts,
        "hard_requirements": [
            "Return JSON only.",
            "Every step must have id, tool, description, input, success_criteria, evidence_ids, citation_ids, formula_ids, risk_ids, validation_gate_ids, and human_checkpoint_ids.",
            "Every step must cite only approved evidence/citation/formula identifiers from planning_context; do not invent identifiers.",
            "Planning without approved evidence is forbidden for production-bound runs.",
            "Include code_generation and test_runner steps.",
            "Include risks and validation gates."
        ]
    }).to_string();
    let plan = call_chat_model_json(state, &state.planner_model, SchemaKey::Planner, system, &user).await?;
    validate_plan_schema(&plan)?;
    planning_context::validate_plan_references(&plan, &planning_context)?;
    Ok(json!({
        "ok": true,
        "kind": "VeritasStructuredPlan",
        "status": "validated_evidence_grounded_structured_plan",
        "goal": goal,
        "model_route": {"planner": role_json(&state.planner_model), "code": role_json(&state.code_model), "math": role_json(&state.math_model)},
        "planning_context": planning_context.clone(),
        "evidence": {"approved_planning_context": planning_context.clone(), "opensearch_faiss_hnsw": evidence, "jena_fuseki_formula_trace": formula_trace, "jena_fuseki_planner_fact_summary": ontology_facts},
        "plan": plan
    }))
}

fn plan_schema_description() -> Value {
    json!({
        "objective": {"summary": "string", "desired_outcome": "string"},
        "steps": [{"id": "string", "tool": "retrieval|sparql|math_reasoning|code_generation|local_command|test_runner", "description": "string", "input": {}, "success_criteria": ["string"], "evidence_ids": ["evidence id"], "citation_ids": ["citation id"], "formula_ids": ["formula id"], "validation_gate_ids": ["validation gate id"], "human_checkpoint_ids": ["plan_review"]}],
        "files_to_generate": [{"path": "relative/path", "purpose": "string"}],
        "commands_to_run": [{"command": "string", "purpose": "string"}],
        "risks": [{"risk": "string", "mitigation": "string"}],
        "validation_gates": [{"id": "validation gate id", "check": "string", "command": "optional string"}]
    })
}


fn schema_description_for(key: SchemaKey) -> Value {
    schema_json(key)
}


fn validate_model_json(schema: SchemaKey, value: &Value) -> Result<(), ApiFailure> {
    validate_json_schema(schema, value).map_err(|errors| ApiFailure::new(
        StatusCode::BAD_GATEWAY,
        "model.schema_invalid",
        format!("Model output failed full JSON Schema validation for {} schema.", schema.as_str()),
        "Use vLLM structured outputs with the role-specific schema, reduce temperature, or choose a stronger model."
    ).with_details(json!({"schema": schema.as_str(), "errors": errors, "output": value})))?;
    match schema {
        SchemaKey::Planner => validate_plan_schema(value),
        SchemaKey::Codegen => validate_codegen_schema(value),
        SchemaKey::MathReasoning => validate_math_reasoning_schema(value),
        SchemaKey::Repair => validate_repair_schema(value),
        SchemaKey::HumanCheckpoint => Ok(()),
        SchemaKey::RunReport => Ok(()),
    }
}

fn generated_path_is_safe(path: &str) -> bool {
    validate_relative_output_path(path).is_ok()
}

fn validate_relative_output_path(path: &str) -> Result<Vec<String>, String> {
    if path.trim().is_empty() {
        return Err("generated path is empty".to_string());
    }
    let path_obj = Path::new(path);
    if path_obj.is_absolute() {
        return Err("generated path must be relative".to_string());
    }
    let mut parts = Vec::new();
    for component in path_obj.components() {
        match component {
            std::path::Component::Normal(value) => {
                let part = value.to_string_lossy().to_string();
                if part.is_empty() || part == "." || part == ".." {
                    return Err(format!("unsafe path component: {part}"));
                }
                parts.push(part);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => return Err("generated path must not contain parent-directory components".to_string()),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return Err("generated path must not contain a root or Windows prefix".to_string()),
        }
    }
    if parts.is_empty() {
        return Err("generated path does not identify a file".to_string());
    }
    Ok(parts)
}

fn validate_repair_schema(output: &Value) -> Result<(), ApiFailure> {
    let mut errors = Vec::new();
    if output.get("failed_command").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("failed_command is required"); }
    if output.get("failure_summary").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("failure_summary is required"); }
    let files = output.get("files").and_then(Value::as_array);
    if files.map(|v| v.is_empty()).unwrap_or(true) { errors.push("files must be a non-empty array"); }
    if let Some(items) = files {
        for item in items {
            if let Some(path) = item.get("path").and_then(Value::as_str) {
                if !generated_path_is_safe(path) { errors.push(format!("unsafe repair file path: {path}")); }
            }
        }
    }
    let commands = output.get("commands").and_then(Value::as_array);
    if commands.is_none() { errors.push("commands array is required"); }
    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "repair.schema_invalid", "Repair model returned JSON that failed the Veritas repair schema.", "Use role-specific structured outputs and retry.").with_details(json!({"errors": errors, "output": output}))) }
}

fn validate_math_reasoning_schema(output: &Value) -> Result<(), ApiFailure> {
    let mut errors = Vec::new();
    let required_arrays = ["axiom_map", "primitive_ontology", "transformation_space", "constraint_geometry", "invariants", "generative_necessity", "symbolic_shadows", "transfer_tests", "risks", "validation_requirements"];
    let required_objects = ["surface_phenomenon", "candidate_representation_map", "compression_fidelity", "recursive_closure"];
    for field in ["summary", "representation_hypothesis", "status"] {
        if output.get(field).and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) {
            errors.push(format!("{field} is required"));
        }
    }
    for field in required_arrays {
        if output.get(field).and_then(Value::as_array).is_none() {
            errors.push(format!("{field} array is required"));
        }
    }
    for field in required_objects {
        if output.get(field).and_then(Value::as_object).is_none() {
            errors.push(format!("{field} object is required"));
        }
    }
    if output.get("invariants").and_then(Value::as_array).map(|v| v.is_empty()).unwrap_or(true) {
        errors.push("at least one invariant candidate or explicit exploratory invariant note is required".to_string());
    }
    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "math.schema_invalid", "Math model returned JSON that failed the Veritas representation-first math reasoning schema.", "Use the math model role with structured outputs enabled. Required fields mirror MATH.md: surface phenomenon, representation map, transformations, invariants, compression fidelity, recursion, necessity, symbolic shadows, transfer, and status.").with_details(json!({"errors": errors, "output": output}))) }
}

fn validate_plan_schema(plan: &Value) -> Result<(), ApiFailure> {
    let mut errors = Vec::new();
    if plan.pointer("/objective/summary").and_then(Value::as_str).is_none() {
        errors.push("objective.summary is required");
    }
    let steps = plan.get("steps").and_then(Value::as_array);
    match steps {
        Some(items) if !items.is_empty() => {
            let mut has_codegen = false;
            let mut has_test = false;
            for (index, step) in items.iter().enumerate() {
                if step.get("id").and_then(Value::as_str).is_none() { errors.push("step.id is required"); }
                let tool = step.get("tool").and_then(Value::as_str).unwrap_or_default();
                if tool.is_empty() { errors.push("step.tool is required"); }
                if tool == "code_generation" { has_codegen = true; }
                if tool == "test_runner" || tool == "local_command" { has_test = true; }
                if step.get("description").and_then(Value::as_str).is_none() { errors.push("step.description is required"); }
                if step.get("success_criteria").and_then(Value::as_array).map(|v| v.is_empty()).unwrap_or(true) {
                    errors.push("step.success_criteria must be non-empty");
                }
                if index > 50 { errors.push("too many plan steps"); }
            }
            if !has_codegen { errors.push("plan must include at least one code_generation step"); }
            if !has_test { errors.push("plan must include at least one test_runner or local_command validation step"); }
        }
        _ => errors.push("steps must be a non-empty array"),
    }
    if plan.get("risks").and_then(Value::as_array).map(|v| v.is_empty()).unwrap_or(true) {
        errors.push("risks must be a non-empty array");
    }
    if plan.get("validation_gates").and_then(Value::as_array).map(|v| v.is_empty()).unwrap_or(true) {
        errors.push("validation_gates must be a non-empty array");
    }
    if let Some(files) = plan.get("files_to_generate").and_then(Value::as_array) {
        for file in files {
            if let Some(path) = file.get("path").and_then(Value::as_str) {
                if !generated_path_is_safe(path) { errors.push(format!("unsafe planned file path: {path}")); }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "plan.schema_invalid", "Planner returned JSON that failed the Veritas plan schema.", "Inspect the planner model output, reduce temperature, or use a stronger planning model.").with_details(json!({"errors": errors, "plan": plan})))
    }
}

async fn execute_autonomous_run(state: &AppState, req: RunRequest) -> Result<Value, ApiFailure> {
    if req.goal.trim().is_empty() {
        return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "run.validation", "Run request goal is empty.", "Provide a non-empty goal."));
    }
    fs::create_dir_all(&state.runs_dir).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.runs_dir_create", format!("Could not create runs directory: {error}"), "Ensure the API container has write access to VERITAS_RUNS_DIR."))?;
    let run_id = format!("run-{}-{}", now_millis(), uuid::Uuid::new_v4().simple());
    let workspace = state.runs_dir.join(&run_id);
    fs::create_dir_all(&workspace).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.workspace_create", format!("Could not create run workspace: {error}"), "Ensure the API container has write access to /workspace/data/runs."))?;
    let _lock = acquire_run_lock(&workspace, &run_id).await?;
    let mut persisted_req: RunResumeRequest = req.clone().into();
    persisted_req.run_id = run_id.clone();
    write_json_file(&workspace.join("request.json"), &json!(persisted_req)).await?;
    persist_run_state(&workspace, "Created", json!({"goal": req.goal, "run_id": run_id})).await?;
    execute_autonomous_run_core(state, run_id, workspace, req, false).await
}

async fn resume_autonomous_run(state: &AppState, run_id: &str, workspace: PathBuf) -> Result<Value, ApiFailure> {
    let _lock = acquire_run_lock(&workspace, run_id).await?;
    let request_value = read_json_file(&workspace.join("request.json")).await
        .ok_or_else(|| ApiFailure::new(StatusCode::CONFLICT, "run.resume.request_missing", format!("Run {run_id} has no request.json."), "Only runs created after Pass 2 durable execution support can be resumed automatically. Start a new run or manually inspect the workspace."))?;
    let request: RunResumeRequest = serde_json::from_value(request_value.clone())
        .map_err(|error| ApiFailure::new(StatusCode::CONFLICT, "run.resume.request_invalid", format!("Run {run_id} request.json is invalid: {error}"), "Inspect request.json or start a new run.").with_details(request_value))?;
    persist_run_state(&workspace, "ResumeRequested", json!({"run_id": run_id, "request": request})).await?;
    execute_autonomous_run_core(state, run_id.to_string(), workspace, request.into_run_request(), true).await
}

async fn execute_autonomous_run_core(state: &AppState, run_id: String, workspace: PathBuf, req: RunRequest, resume: bool) -> Result<Value, ApiFailure> {
    if workspace.join("CANCELLED").exists() {
        persist_run_state(&workspace, "Cancelled", json!({"reason": "cancel marker existed before execution"})).await?;
        return Err(ApiFailure::new(StatusCode::CONFLICT, "run.cancelled", format!("Run {run_id} is cancelled."), "Start a new run, or remove the cancellation marker only if you intentionally want to retry this exact workspace."));
    }

    let language = req.language.clone().unwrap_or_else(|| "rust".to_string());
    let max_retries = req.max_retries.unwrap_or(state.max_retries).min(5);
    apply_preloaded_artifacts(&workspace, req.preloaded_artifacts.as_ref()).await?;
    let plan_envelope_path = workspace.join("plan_envelope.json");
    let plan_envelope = if resume && plan_envelope_path.exists() {
        read_json_file(&plan_envelope_path).await.ok_or_else(|| ApiFailure::new(StatusCode::CONFLICT, "run.resume.plan_unreadable", "The persisted plan_envelope.json could not be read.", "Inspect or delete the corrupted plan file before resuming."))?
    } else {
        let run_planning_context = json!({
            "execution_mode": req.execution_mode.clone().unwrap_or_else(|| "production".to_string()),
            "preloaded_artifacts_present": req.preloaded_artifacts.is_some()
        });
        let envelope = build_structured_plan(state, &req.goal, req.size.unwrap_or(8), Some(&workspace), Some(&run_planning_context)).await?;
        write_json_file(&plan_envelope_path, &envelope).await?;
        persist_run_state(&workspace, "Planned", json!({"plan_envelope_path": plan_envelope_path.display().to_string(), "planning_context_path": "planning_context.json", "resumed": resume})).await?;
        envelope
    };
    let plan = plan_envelope.get("plan").cloned().unwrap_or_else(|| json!({}));
    write_json_file(&workspace.join("plan.json"), &plan).await?;
    let lineage_context = lineage::build_lineage_context(&workspace, &plan_envelope, &plan).await?;
    lineage::write_planning_context(&workspace, &lineage_context).await?;
    lineage::validate_plan_lineage(&plan, &lineage_context)?;
    persist_run_state(&workspace, "LineageContextReady", json!({"planning_context": "planning_context.json"})).await?;

    let mut tool_calls: Vec<Value> = read_json_file(&workspace.join("tool_calls.json")).await.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let tool_outputs_path = workspace.join("tool_outputs.json");
    let tool_outputs = if resume && tool_outputs_path.exists() {
        read_json_file(&tool_outputs_path).await.unwrap_or_else(|| json!([]))
    } else {
        let outputs = execute_planner_selected_tools(state, &plan, &req.goal, &mut tool_calls).await;
        write_json_file(&tool_outputs_path, &outputs).await?;
        write_json_file(&workspace.join("tool_calls.json"), &json!(tool_calls)).await?;
        outputs
    };

    let automatic_shacl_path = workspace.join("automatic_shacl_report.json");
    let automatic_shacl = if resume && automatic_shacl_path.exists() {
        read_json_file(&automatic_shacl_path).await.unwrap_or_else(|| json!({"ok": false, "warning": "existing SHACL report unreadable"}))
    } else {
        let report = automatic_shacl_gate(state, &workspace, &plan).await?;
        write_json_file(&automatic_shacl_path, &report).await?;
        persist_run_state(&workspace, "ShaclValidated", json!({"automatic_shacl_path": automatic_shacl_path.display().to_string(), "resumed": resume})).await?;
        report
    };

    if let Some(math_report) = math_tools::validate_workspace_if_required(state, &workspace, &req.goal, &plan).await? {
        persist_run_state(&workspace, "MathToolsValidated", json!({"math_validation_report": "math_validation_report.json", "ok": math_report.get("ok").and_then(Value::as_bool).unwrap_or(false)})).await?;
    }

    let pre_codegen_gates = match gates::run_pre_codegen_gates(state, &workspace, &run_id, &req.goal, &plan, &automatic_shacl).await {
        Ok(report) => {
            persist_run_state(&workspace, "PreCodegenGatesPassed", json!({"pre_codegen_gate_report": "pre_codegen_gate_report.json"})).await?;
            report
        }
        Err(error) => {
            let blocked_report = gates::write_pre_codegen_blocked_report(state, &workspace, &run_id, &req.goal, &language, &plan, error).await?;
            return Ok(blocked_report);
        }
    };

    let mut files_changed: Vec<String> = read_json_file(&workspace.join("files_changed.json")).await
        .and_then(|v| v.as_array().cloned())
        .map(|items| items.into_iter().filter_map(|v| v.as_str().map(ToString::to_string)).collect())
        .unwrap_or_default();
    let mut commands_run: Vec<Value> = read_json_file(&workspace.join("commands_run.json")).await.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let mut validation_results: Vec<Value> = read_json_file(&workspace.join("validation_results.json")).await.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let mut retry_history: Vec<Value> = read_json_file(&workspace.join("retry_history.json")).await.and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let mut last_error_summary = validation_results.last().cloned().unwrap_or_else(|| json!({})).to_string();
    let mut code_package = read_json_file(&workspace.join("code_package_latest.json")).await.unwrap_or_else(|| json!({}));
    let mut final_status = "failed".to_string();
    let mut attempts_performed = validation_results.len();
    let start_attempt = attempts_performed.min(max_retries + 1);

    for attempt in start_attempt..=max_retries {
        if workspace.join("CANCELLED").exists() {
            final_status = "cancelled".to_string();
            persist_run_state(&workspace, "Cancelled", json!({"attempt": attempt})).await?;
            break;
        }
        attempts_performed = attempt + 1;
        let code_prompt = build_code_generation_prompt(&req.goal, &language, &plan, &plan_envelope["evidence"], &tool_outputs, &lineage_context, &last_error_summary, attempt);
        let generated = call_chat_model_json(state, &state.code_model, SchemaKey::Codegen, codegen_system_prompt(), &code_prompt).await?;
        validate_codegen_schema(&generated)?;
        lineage::validate_codegen_lineage_for_plan(&plan, &lineage_context, &generated)?;
        persist_run_state(&workspace, "GeneratingCode", json!({"attempt": attempt, "resumed": resume, "lineage_validated": true})).await?;
        write_json_file(&workspace.join(format!("code_package_attempt_{attempt}.json")), &generated).await?;
        let file_lineage = lineage::write_file_lineage(&workspace, &generated).await?;
        write_json_file(&workspace.join(format!("file_lineage_attempt_{attempt}.json")), &file_lineage).await?;
        write_generated_files(&workspace, &generated, &mut files_changed).await?;
        code_package = generated.clone();
        write_json_file(&workspace.join("code_package_latest.json"), &code_package).await?;
        write_json_file(&workspace.join("files_changed.json"), &json!(files_changed)).await?;

        let commands = commands_for_run(&workspace, &language, &generated);
        let mut all_passed = true;
        let mut attempt_results = Vec::new();
        for command in commands {
            if workspace.join("CANCELLED").exists() {
                all_passed = false;
                final_status = "cancelled".to_string();
                persist_run_state(&workspace, "Cancelled", json!({"attempt": attempt, "during": "validation"})).await?;
                break;
            }
            persist_run_state(&workspace, "RunningValidation", json!({"attempt": attempt, "command": command})).await?;
            let result = run_command(&workspace, &command, state.command_timeout_secs).await;
            append_command_audit(&workspace, &result).await?;
            commands_run.push(result.clone());
            attempt_results.push(result.clone());
            if !result.get("success").and_then(Value::as_bool).unwrap_or(false) {
                all_passed = false;
            }
        }
        write_json_file(&workspace.join("commands_run.json"), &json!(commands_run)).await?;
        validation_results.push(json!({"attempt": attempt, "results": attempt_results}));
        write_json_file(&workspace.join("validation_results.json"), &json!(validation_results)).await?;
        if all_passed {
            final_status = "production_candidate_validated".to_string();
            persist_run_state(&workspace, "Validated", json!({"attempt": attempt})).await?;
            break;
        }
        if final_status == "cancelled" { break; }
        last_error_summary = validation_results.last().cloned().unwrap_or_else(|| json!({})).to_string();
        persist_run_state(&workspace, "Repairing", json!({"attempt": attempt, "failure_summary": last_error_summary})).await?;
        retry_history.push(json!({
            "attempt": attempt,
            "reason": "compile_or_test_failure",
            "feedback_sent_to_code_model": last_error_summary
        }));
        write_json_file(&workspace.join("retry_history.json"), &json!(retry_history)).await?;
    }

    let final_shacl_report = run_shacl_gate(state, &workspace, &plan, "final_artifact_shacl").await?;
    write_json_file(&workspace.join("final_artifact_shacl_report.json"), &final_shacl_report).await?;
    if state.governance_mode.enforces() && !shacl_report_conforms(&final_shacl_report) {
        final_status = "blocked_by_governance".to_string();
        persist_run_state(&workspace, "FinalShaclBlocked", json!({"final_artifact_shacl_report": "final_artifact_shacl_report.json"})).await?;
    }

    let provider_route_history = state.provider_router.history_snapshot().await;
    let human_checkpoints = read_events_tail(&workspace.join("human_checkpoints.jsonl"), 500).await.unwrap_or_default();
    let human_checkpoint_gate = human_checkpoint_gate_summary(&workspace, &state.human_loop_policy).await;
    let artifact_decision = artifact_decision::decide_completed_run(
        state,
        &workspace,
        &run_id,
        &final_status,
        &pre_codegen_gates,
        &final_shacl_report,
        &human_checkpoint_gate,
        &validation_results,
        &retry_history,
        &commands_run,
        &files_changed,
        workspace.join("CANCELLED").exists(),
    ).await?;
    let artifact_status = artifact_decision.get("artifact_status").and_then(Value::as_str).unwrap_or("failed").to_string();
    let remaining_limitations = artifact_decision.get("remaining_limitations").cloned().unwrap_or_else(|| json!([]));
    let report_lineage = lineage::build_report_lineage(&workspace, &plan_envelope, &plan, &code_package, &commands_run, &validation_results, &retry_history, &artifact_decision).await?;
    let mut report = json!({
        "ok": artifact_decision.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "kind": "VeritasAutonomousRunReport",
        "run_id": run_id,
        "workspace": workspace.display().to_string(),
        "original_task": req.goal,
        "source_documents": report_lineage.get("source_documents").cloned().unwrap_or_else(|| json!([])),
        "citations": report_lineage.get("citations").cloned().unwrap_or_else(|| json!([])),
        "formulas": report_lineage.get("formulas").cloned().unwrap_or_else(|| json!([])),
        "review_decisions": report_lineage.get("review_decisions").cloned().unwrap_or_else(|| json!({})),
        "representation_model": report_lineage.get("representation_model").cloned().unwrap_or_else(|| json!(null)),
        "planning_context": report_lineage.get("planning_context").cloned().unwrap_or_else(|| json!({})),
        "plan_lineage": report_lineage.get("plan_lineage").cloned().unwrap_or_else(|| json!({})),
        "file_lineage": report_lineage.get("file_lineage").cloned().unwrap_or_else(|| json!({"files": []})),
        "command_lineage": report_lineage.get("command_lineage").cloned().unwrap_or_else(|| json!({})),
        "validation_lineage": report_lineage.get("validation_lineage").cloned().unwrap_or_else(|| json!({})),
        "repair_lineage": report_lineage.get("repair_lineage").cloned().unwrap_or_else(|| json!([])),
        "governance_lineage": report_lineage.get("governance_lineage").cloned().unwrap_or_else(|| json!({})),
        "language": language,
        "generated_plan": plan,
        "model_routes_used": {"planner": role_json(&state.planner_model), "code": role_json(&state.code_model), "math": role_json(&state.math_model)},
        "provider_route_history": provider_route_history,
        "tool_calls_performed": tool_calls,
        "automatic_shacl": automatic_shacl,
        "final_shacl": final_shacl_report,
        "governance_mode": state.governance_mode.as_str(),
        "pre_codegen_gates": pre_codegen_gates,
        "human_checkpoint_policy": state.human_loop_policy.clone(),
        "human_checkpoints": human_checkpoints,
        "human_checkpoint_gate": human_checkpoint_gate,
        "files_changed": files_changed,
        "commands_run": commands_run,
        "validation_results": validation_results,
        "attempts_performed": attempts_performed,
        "retries_performed": retry_history.len(),
        "retry_history": retry_history,
        "generated_package_status": artifact_status.clone(),
        "artifact_status": artifact_status.clone(),
        "artifact_decision": artifact_decision,
        "final_status": artifact_status,
        "resumed": resume,
        "remaining_limitations": remaining_limitations,
        "code_model_output": code_package,
    });
    let graph_upload = upload_run_report_to_fuseki(state, &run_id, &report).await.unwrap_or_else(|error| json!({"ok": false, "code": error.code, "message": error.message, "remediation": error.remediation}));
    report["fuseki_run_graph_upload"] = graph_upload;
    validate_model_json(SchemaKey::RunReport, &report)?;
    write_json_file(&workspace.join("final_report.json"), &report).await?;
    persist_run_state(&workspace, "FinalReportWritten", json!({"final_status": report.get("final_status"), "artifact_decision": "artifact_decision.json", "report": "final_report.json"})).await?;
    Ok(report)
}

async fn execute_planner_selected_tools(state: &AppState, plan: &Value, goal: &str, tool_calls: &mut Vec<Value>) -> Value {
    let mut outputs = Vec::new();
    if let Some(steps) = plan.get("steps").and_then(Value::as_array) {
        for step in steps {
            let tool = step.get("tool").and_then(Value::as_str).unwrap_or_default();
            match tool {
                "retrieval" => {
                    let query = step.pointer("/input/query").and_then(Value::as_str).unwrap_or(goal);
                    let result = retrieve_evidence(state, query, 5, Some("hybrid")).await;
                    tool_calls.push(json!({"tool": "retrieval", "query": query, "success": result.is_ok()}));
                    outputs.push(json!({"tool": "retrieval", "step": step, "result": result.ok()}));
                }
                "sparql" => {
                    let query = step.pointer("/input/sparql_query").and_then(Value::as_str).unwrap_or(FORMULA_TRACE_QUERY);
                    let result = run_sparql(state, query).await;
                    tool_calls.push(json!({"tool": "sparql", "success": result.is_ok()}));
                    outputs.push(json!({"tool": "sparql", "step": step, "result": result.ok()}));
                }
                "shacl_validate" => {
                    let data_ttl = step.pointer("/input/data_ttl").and_then(Value::as_str).unwrap_or("");
                    let shapes_ttl = step.pointer("/input/shapes_ttl").and_then(Value::as_str).unwrap_or("");
                    let result = run_shacl_validation(state, data_ttl, shapes_ttl).await;
                    tool_calls.push(json!({"tool": "shacl_validate", "success": result.is_ok()}));
                    outputs.push(json!({"tool": "shacl_validate", "step": step, "result": result.ok()}));
                }
                "math_reasoning" => {
                    let input = step.get("input").cloned().unwrap_or_else(|| json!({"goal": goal}));
                    let prompt = build_math_to_code_reasoning_prompt(&json!({"planner_step_input": input, "goal": goal}), "analysis");
                    let result = call_chat_model_json(state, &state.math_model, SchemaKey::MathReasoning, math_to_code_system_prompt(), &prompt).await;
                    tool_calls.push(json!({"tool": "math_reasoning", "schema": "math_reasoning", "success": result.is_ok()}));
                    outputs.push(json!({"tool": "math_reasoning", "step": step, "result": result.ok()}));
                }
                "code_generation" | "test_runner" | "local_command" => {
                    tool_calls.push(json!({"tool": tool, "deferred": true, "reason": "executed during code generation and validation phase"}));
                }
                _ => tool_calls.push(json!({"tool": tool, "skipped": true, "reason": "unknown tool name"})),
            }
        }
    }
    json!(outputs)
}

fn build_code_generation_prompt(goal: &str, language: &str, plan: &Value, evidence: &Value, tool_outputs: &Value, lineage_context: &Value, last_error_summary: &str, attempt: usize) -> String {
    json!({
        "goal": goal,
        "language": language,
        "attempt": attempt,
        "plan": plan,
        "evidence": evidence,
        "tool_outputs": tool_outputs,
        "lineage_context": lineage_context,
        "lineage_contract": lineage::codegen_lineage_contract(plan, lineage_context),
        "previous_failure_summary": last_error_summary,
        "functional_programming_policy": {
            "referential_transparency": true,
            "pure_domain_core": true,
            "side_effect_boundaries": true,
            "small_composable_functions": true,
            "higher_order_functions": true,
            "dispatch_tables": true,
            "immutable_data_where_practical": true
        },
        "required_json_schema": {
            "package_name": "snake_case string",
            "language": language,
            "files": [{"path": "relative/path", "content": "complete file content", "purpose": "why this file exists", "derived_from_plan_step_ids": ["plan step id"], "derived_from_evidence_ids": ["evidence id"], "derived_from_citation_ids": ["citation id"], "derived_from_formula_ids": ["formula id"], "required_validation_ids": ["validation gate id"]}],
            "commands": [{"command": "shell command to compile/test in workspace", "purpose": "why this validates the output", "derived_from_plan_step_ids": ["plan step id"], "required_validation_ids": ["validation gate id"]}],
            "assumptions": ["string"],
            "validation_summary": "string"
        },
        "hard_requirements": [
            "Return JSON only. No markdown outside JSON.",
            "Generate complete files, not snippets.",
            "For Rust, include Cargo.toml, src/lib.rs, and at least one test.",
            "Commands must compile and run tests.",
            "Every generated file and command must include lineage ids from lineage_contract; the application rejects unknown or empty ids before writing files.",
            "Do not claim GPU support unless actual GPU code is generated and tested. Prefer CPU-safe implementation with explicit extension points.",
            "Every generated file must include derived_from_plan_step_ids, derived_from_evidence_ids, derived_from_citation_ids, derived_from_formula_ids, and required_validation_ids from lineage_contract.",
            "Every validation command must include derived_from_plan_step_ids and required_validation_ids from lineage_contract."
        ]
    }).to_string()
}

fn codegen_system_prompt() -> &'static str {
    "You are Veritas Code Writer. Return valid JSON only. Generate production-oriented, testable, maintainable code. Use functional-composition style: pure functions, immutable data where practical, explicit side-effect boundaries, dispatch tables for strategy routing, small reusable functions, and clear tests. Do not emit incomplete files or TODO-only files."
}

fn validate_codegen_schema(output: &Value) -> Result<(), ApiFailure> {
    let mut errors = Vec::new();
    if output.get("package_name").and_then(Value::as_str).is_none() { errors.push("package_name is required"); }
    let files = output.get("files").and_then(Value::as_array);
    match files {
        Some(items) if !items.is_empty() => {
            for item in items {
                if item.get("path").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("each file needs a non-empty path"); }
                if let Some(path) = item.get("path").and_then(Value::as_str) {
                    if !generated_path_is_safe(path) { errors.push(format!("unsafe generated file path: {path}")); }
                }
                if item.get("content").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("each file needs non-empty content"); }
                for field in ["purpose", "derived_from_plan_step_ids", "derived_from_evidence_ids", "derived_from_citation_ids", "derived_from_formula_ids", "required_validation_ids"] {
                    match item.get(field) {
                        Some(Value::String(s)) if field == "purpose" && !s.trim().is_empty() => {}
                        Some(Value::Array(items)) if field != "purpose" && items.iter().all(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)) => {}
                        _ => errors.push(format!("each file needs valid {field}")),
                    }
                }
            }
        }
        _ => errors.push("files must be a non-empty array"),
    }
    match output.get("commands").and_then(Value::as_array) {
        Some(commands) => {
            for command in commands {
                if command.get("command").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("each command needs a non-empty command"); }
                if command.get("purpose").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("each command needs a non-empty purpose"); }
                for field in ["derived_from_plan_step_ids", "required_validation_ids"] {
                    match command.get(field) {
                        Some(Value::Array(items)) if items.iter().all(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)) => {}
                        _ => errors.push(format!("each command needs valid {field}")),
                    }
                }
            }
        }
        None => errors.push("commands must be an array"),
    }
    if let Some(commands) = output.get("commands").and_then(Value::as_array) {
        for command in commands {
            for lineage_field in ["purpose", "derived_from_plan_step_ids", "required_validation_ids"] {
                if command.get(lineage_field).is_none() { errors.push(format!("each command needs {lineage_field} for validation lineage")); }
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.schema_invalid", "Code model returned JSON that failed the Veritas codegen schema.", "Use a stronger code model or reduce temperature; inspect model output in the error details.").with_details(json!({"errors": errors, "output": output}))) }
}

async fn write_generated_files(workspace: &Path, generated: &Value, files_changed: &mut Vec<String>) -> Result<(), ApiFailure> {
    let files = generated.get("files").and_then(Value::as_array).ok_or_else(|| ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.files_missing", "Code model output contained no files.", "Retry with a model that follows the Veritas JSON schema."))?;
    for file in files {
        let rel = file.get("path").and_then(Value::as_str).unwrap_or_default();
        let content = file.get("content").and_then(Value::as_str).unwrap_or_default();
        let target = safe_output_path(workspace, rel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.mkdir", format!("Could not create directory for {rel}: {error}"), "Check run workspace permissions."))?;
            verify_existing_path_inside_workspace(workspace, parent, "generated file parent").await?;
        }
        reject_existing_symlink(&target, rel).await?;
        let tmp = target.with_extension("veritas_tmp");
        fs::write(&tmp, content).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.write_tmp", format!("Could not write temporary generated file {rel}: {error}"), "Check run workspace permissions."))?;
        fs::rename(&tmp, &target).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.atomic_write", format!("Could not atomically write generated file {rel}: {error}"), "Check run workspace permissions and filesystem semantics."))?;
        reject_existing_symlink(&target, rel).await?;
        verify_existing_path_inside_workspace(workspace, &target, "generated file").await?;
        files_changed.push(rel.to_string());
    }
    Ok(())
}

fn safe_output_path(root: &Path, rel: &str) -> Result<PathBuf, ApiFailure> {
    let parts = validate_relative_output_path(rel).map_err(|reason| ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.unsafe_path", format!("Generated file path is unsafe: {rel}: {reason}"), "Regenerate code. Generated paths must be relative file paths without absolute roots, parent-directory components, or path prefixes."))?;
    let root_canonical = std::fs::canonicalize(root).map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.workspace_canonicalize", format!("Could not canonicalize run workspace {}: {error}", root.display()), "Check run workspace permissions and ensure it exists before writing files."))?;
    let mut cursor = root_canonical.clone();
    for part in &parts[..parts.len().saturating_sub(1)] {
        cursor.push(Path::new(part.as_str()));
        if let Ok(metadata) = std::fs::symlink_metadata(&cursor) {
            if metadata.file_type().is_symlink() {
                return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.symlink_parent", format!("Generated path {rel} traverses an existing symlink parent: {}", cursor.display()), "Remove the symlink or choose a safe path inside the run workspace."));
            }
        }
    }
    let mut target = root_canonical;
    for part in parts {
        target.push(Path::new(part.as_str()));
    }
    Ok(target)
}

async fn reject_existing_symlink(path: &Path, rel: &str) -> Result<(), ApiFailure> {
    match fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.symlink_target", format!("Generated file path {rel} resolves to an existing symlink."), "Remove the symlink or regenerate into a normal file inside the run workspace.")),
        Ok(_) | Err(_) => Ok(()),
    }
}

async fn verify_existing_path_inside_workspace(root: &Path, path: &Path, label: &str) -> Result<(), ApiFailure> {
    let root_canonical = fs::canonicalize(root).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.workspace_canonicalize", format!("Could not canonicalize workspace {}: {error}", root.display()), "Check run workspace permissions."))?;
    let path_canonical = fs::canonicalize(path).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.path_canonicalize", format!("Could not canonicalize {label} {}: {error}", path.display()), "Check generated file path permissions."))?;
    if !path_canonical.starts_with(&root_canonical) {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.path_escape", format!("Generated {label} escaped the run workspace: {}", path_canonical.display()), "Regenerate code using paths contained by the run workspace."));
    }
    Ok(())
}

fn commands_for_run(workspace: &Path, language: &str, generated: &Value) -> Vec<String> {
    let mut commands: Vec<String> = generated
        .get("commands")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|item| item.get("command").and_then(Value::as_str).map(ToString::to_string)).collect())
        .unwrap_or_default();
    if commands.is_empty() {
        if language.eq_ignore_ascii_case("rust") || workspace.join("Cargo.toml").exists() {
            commands.push("cargo test".to_string());
        } else if workspace.join("pyproject.toml").exists() || workspace.join("pytest.ini").exists() {
            commands.push("python3 -m pytest".to_string());
        }
    }
    commands
}

async fn run_command(workspace: &Path, command: &str, timeout_secs: u64) -> Value {
    let runner = effective_command_runner();
    if runner == "docker" || runner == "sandbox" || runner == "docker_sandbox" {
        run_command_in_docker_sandbox(workspace, command, timeout_secs).await
    } else if runner == "local" {
        if !local_command_runner_allowed() {
            return json!({
                "command": command,
                "success": false,
                "runner": "local_blocked",
                "duration_ms": 0,
                "error": "local command runner is disabled for the active production profile",
                "remediation": "Use VERITAS_COMMAND_RUNNER=sandbox/docker, or explicitly set VERITAS_ALLOW_LOCAL_COMMAND_RUNNER=true only for trusted development environments.",
                "profile": active_veritas_profile()
            });
        }
        run_command_local(workspace, command, timeout_secs).await
    } else {
        json!({
            "command": command,
            "success": false,
            "runner": runner,
            "duration_ms": 0,
            "error": "unknown command runner",
            "remediation": "Set VERITAS_COMMAND_RUNNER to sandbox, docker, docker_sandbox, or local. Production profiles default to sandbox."
        })
    }
}

fn active_veritas_profile() -> String {
    env::var("VERITAS_PROFILE")
        .or_else(|_| env::var("VERITAS_ACCEPTANCE_PROFILE"))
        .unwrap_or_else(|_| "development".to_string())
}

fn is_production_profile() -> bool {
    let profile = active_veritas_profile().to_ascii_lowercase();
    matches!(profile.as_str(), "production" | "prod" | "host-prod" | "single-gpu-prod" | "multi-gpu-prod" | "remote-model-prod") || profile.ends_with("-prod")
}

fn effective_command_runner() -> String {
    match env::var("VERITAS_COMMAND_RUNNER") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_ascii_lowercase(),
        _ if is_production_profile() => "sandbox".to_string(),
        _ => "local".to_string(),
    }
}

fn local_command_runner_allowed() -> bool {
    !is_production_profile() || bool_env("VERITAS_ALLOW_LOCAL_COMMAND_RUNNER", false)
}

async fn run_command_local(workspace: &Path, command: &str, timeout_secs: u64) -> Value {
    let started = now_millis();
    if let Some(reason) = command_rejection_reason(command) {
        return json!({"command": command, "success": false, "runner": "local", "duration_ms": 0, "error": "command rejected by Veritas allowlist", "reason": reason, "remediation": "Use cargo test/check/fmt/clippy, python -m pytest/build, ruff, mypy, cmake, or ctest unless the operator explicitly extends the allowlist."});
    }
    let result = timeout(Duration::from_secs(timeout_secs), Command::new("sh").arg("-lc").arg(command).current_dir(workspace).output()).await;
    command_output_json("local", command, started, timeout_secs, result)
}

async fn run_command_in_docker_sandbox(workspace: &Path, command: &str, timeout_secs: u64) -> Value {
    let started = now_millis();
    if let Some(reason) = command_rejection_reason(command) {
        return json!({"command": command, "success": false, "runner": "docker_sandbox", "duration_ms": 0, "error": "command rejected by Veritas allowlist", "reason": reason, "remediation": "Only approved compile/test commands may run in the sandbox."});
    }
    let image = env::var("VERITAS_SANDBOX_IMAGE").unwrap_or_else(|_| "veritas-sandbox-rust:latest".to_string());
    let memory = env::var("VERITAS_SANDBOX_MEMORY").unwrap_or_else(|_| "2g".to_string());
    let cpus = env::var("VERITAS_SANDBOX_CPUS").unwrap_or_else(|_| "2".to_string());
    let pids_limit = env::var("VERITAS_SANDBOX_PIDS_LIMIT").unwrap_or_else(|_| "256".to_string());
    let tmpfs_size = env::var("VERITAS_SANDBOX_TMPFS_SIZE").unwrap_or_else(|_| "256m".to_string());
    let mount = format!("{}:/workspace:rw", workspace.display());
    let tmpfs = format!("/tmp:rw,noexec,nosuid,size={tmpfs_size}");
    let result = timeout(
        Duration::from_secs(timeout_secs),
        Command::new("docker")
            .arg("run").arg("--rm")
            .arg("--network").arg("none")
            .arg("--cpus").arg(cpus)
            .arg("--memory").arg(memory)
            .arg("--pids-limit").arg(pids_limit)
            .arg("--cap-drop").arg("ALL")
            .arg("--security-opt").arg("no-new-privileges")
            .arg("--read-only")
            .arg("--tmpfs").arg(tmpfs)
            .arg("-v").arg(mount)
            .arg("-w").arg("/workspace")
            .arg(image)
            .arg("sh").arg("-lc").arg(command)
            .output(),
    ).await;
    command_output_json("docker_sandbox", command, started, timeout_secs, result)
}

fn command_output_json(runner: &str, command: &str, started: u128, timeout_secs: u64, result: Result<Result<std::process::Output, std::io::Error>, tokio::time::error::Elapsed>) -> Value {
    match result {
        Ok(Ok(output)) => json!({"command": command, "runner": runner, "success": output.status.success(), "exit_code": output.status.code(), "duration_ms": now_millis().saturating_sub(started), "stdout": truncate_output(&String::from_utf8_lossy(&output.stdout)), "stderr": truncate_output(&String::from_utf8_lossy(&output.stderr))}),
        Ok(Err(error)) => json!({"command": command, "runner": runner, "success": false, "duration_ms": now_millis().saturating_sub(started), "error": format!("Failed to launch command: {error}"), "remediation": "Ensure Docker and the selected sandbox image/toolchain are available."}),
        Err(_) => json!({"command": command, "runner": runner, "success": false, "duration_ms": now_millis().saturating_sub(started), "error": format!("Command timed out after {timeout_secs}s"), "remediation": "Increase VERITAS_COMMAND_TIMEOUT_SECS or simplify generated tests."}),
    }
}

fn truncate_output(text: &str) -> String {
    let limit = uint_env("VERITAS_COMMAND_OUTPUT_LIMIT", 20000) as usize;
    if text.len() <= limit { text.to_string() } else { format!("{}\n... [truncated to last {limit} chars]", &text[text.len().saturating_sub(limit)..]) }
}

async fn retrieve_evidence(state: &AppState, query: &str, size: u32, mode: Option<&str>) -> Result<Value, ApiFailure> {
    if query.trim().is_empty() { return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "search.validation", "Search query is empty.", "Provide a non-empty query string.")); }
    let mode = mode.unwrap_or("hybrid");
    let body = if mode == "lexical" {
        lexical_query(query, size)
    } else {
        let vector = embed_query(state, query).await?;
        if mode == "semantic" { vector_query(&state.opensearch_vector_field, vector, size) } else { hybrid_query(query, &state.opensearch_vector_field, vector, size) }
    };
    let primary_target = &state.opensearch_index;
    match post_opensearch_search(state, primary_target, &body).await {
        Ok(mut value) => {
            value["veritas_search_target"] = json!(primary_target);
            Ok(value)
        }
        Err(error) => {
            let retry_targets = [&state.opensearch_read_alias, &state.opensearch_write_alias, &state.opensearch_base_index, &state.opensearch_versioned_index];
            let mut attempts = vec![json!({"target": primary_target, "error": {"code": error.code, "message": error.message}})];
            for target in retry_targets {
                if target == primary_target { continue; }
                match post_opensearch_search(state, target, &body).await {
                    Ok(mut value) => {
                        value["veritas_search_target"] = json!(target);
                        value["veritas_search_fallback_attempts"] = json!(attempts);
                        return Ok(value);
                    }
                    Err(next_error) => attempts.push(json!({"target": target, "error": {"code": next_error.code, "message": next_error.message}})),
                }
            }
            Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "search.all_targets_failed", "OpenSearch search failed across configured read alias, write alias, base index, and versioned index.", "Run `veritas opensearch-migrate`, ingest documents, then retry search. Inspect OpenSearch logs if aliases are missing.").with_details(json!({"attempts": attempts})))
        }
    }
}

async fn post_opensearch_search(state: &AppState, target: &str, body: &Value) -> Result<Value, ApiFailure> {
    let url = format!("{}/{}/_search", state.opensearch_url.trim_end_matches('/'), target);
    let response = state.http.post(&url).json(body).send().await.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "search.transport", format!("OpenSearch request failed before the service returned a response for target {target}: {error}"), "Check OpenSearch readiness with `veritas ready` and inspect `docker compose logs opensearch`."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "search.upstream", format!("OpenSearch target {target} returned HTTP {}", status.as_u16()), "Check OpenSearch logs, index mapping, aliases, and whether data has been ingested.").with_details(body));
    }
    Ok(body)
}

fn lexical_query(query: &str, size: u32) -> Value {
    json!({
        "size": size,
        "query": {"bool": {"should": [
            {"multi_match": {"query": query, "fields": ["text^3", "title^4", "metadata.summary^2"]}},
            {"nested": {"path": "formulas", "query": {"match": {"formulas.latex": query}}, "score_mode": "max"}}
        ], "minimum_should_match": 1}}
    })
}

fn hybrid_query(query: &str, vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(vector_field.to_string(), json!({"vector": vector, "k": size}));
    json!({"size": size, "query": {"bool": {"should": [
        {"knn": Value::Object(knn_body)},
        {"multi_match": {"query": query, "fields": ["text^3", "title^4", "metadata.summary^2"]}},
        {"nested": {"path": "formulas", "query": {"match": {"formulas.latex": query}}, "score_mode": "max"}}
    ], "minimum_should_match": 1}}})
}

fn vector_query(vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(vector_field.to_string(), json!({"vector": vector, "k": size}));
    json!({"size": size, "query": {"knn": Value::Object(knn_body)}})
}

async fn embed_query(state: &AppState, query: &str) -> Result<Vec<f32>, ApiFailure> {
    let url = format!("{}/embed", state.embedding_url.trim_end_matches('/'));
    let response = state.http.post(&url).json(&json!({"texts": [query], "normalize": true, "batch_size": 1})).send().await.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "embedding.transport", format!("Embedding request failed before the service returned a response: {error}"), "Check `docker compose logs embedding` and ensure the embedding service is healthy."))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "embedding.upstream", format!("Embedding service returned HTTP {}: {text}", status.as_u16()), "Inspect the embedding service logs and retry with a non-empty query."));
    }
    let payload: EmbedResponse = response.json().await.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "embedding.parse", format!("Embedding response could not be decoded: {error}"), "Check embedding service compatibility and response schema."))?;
    let vector = payload.vectors.into_iter().next().ok_or_else(|| ApiFailure::new(StatusCode::BAD_GATEWAY, "embedding.empty_vector", "Embedding service returned no query vector.", "Retry and inspect `docker compose logs embedding`."))?;
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if (norm - 1.0).abs() > 0.001 {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "embedding.norm", format!("Query embedding is not normalized for cosine search: norm={norm:.6}"), "Ensure VERITAS_EMBEDDING_NORMALIZE=true and the embedding service uses normalize_embeddings=True."));
    }
    Ok(vector)
}

const FORMULA_TRACE_QUERY: &str = r#"
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
PREFIX dcterms: <http://purl.org/dc/terms/>
SELECT ?formula ?expr ?chunk ?paper ?title
WHERE {
  ?formula a veritas:SymbolicShadow ;
           veritas:hasExpressionText ?expr ;
           veritas:derivedFrom ?chunk .
  OPTIONAL { ?chunk veritas:derivedFrom ?paper . }
  OPTIONAL { ?paper dcterms:title ?title . }
}
LIMIT 25
"#;

async fn run_formula_trace_query(state: &AppState) -> Result<Value, ApiFailure> {
    run_sparql(state, FORMULA_TRACE_QUERY).await
}

async fn run_sparql(state: &AppState, query: &str) -> Result<Value, ApiFailure> {
    if query.trim().is_empty() { return Err(ApiFailure::new(StatusCode::BAD_REQUEST, "sparql.validation", "SPARQL query is empty.", "Provide a non-empty SPARQL query string.")); }
    let response = state.http.post(&state.fuseki_query_url).header("accept", "application/sparql-results+json").form(&[("query", query.to_string())]).send().await.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "sparql.transport", format!("Fuseki request failed before the service returned a response: {error}"), "Check Fuseki readiness with `veritas ready` and inspect `docker compose logs fuseki`."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "sparql.upstream", format!("Fuseki returned HTTP {}", status.as_u16()), "Check service logs, endpoint configuration, and whether data has been ingested.").with_details(body));
    }
    Ok(body)
}

async fn call_chat_model_text(state: &AppState, role: &ModelRole, system: &str, user: &str) -> Result<Value, ApiFailure> {
    let raw = call_chat_model_raw(state, role, system, user, None).await?;
    let content = extract_model_content(&raw).unwrap_or_else(|| raw.to_string());
    Ok(json!({"raw": raw, "content": content}))
}

async fn call_chat_model_json(state: &AppState, role: &ModelRole, schema: SchemaKey, system: &str, user: &str) -> Result<Value, ApiFailure> {
    let raw = call_chat_model_raw(state, role, system, user, Some(schema)).await?;
    let content = extract_model_content(&raw).unwrap_or_else(|| raw.to_string());
    match parse_json_object_from_text(&content) {
        Ok(value) => { validate_model_json(schema, &value)?; Ok(value) },
        Err(first_error) => {
            let repair_user = json!({"invalid_output": content, "parse_error": first_error, "instruction": "Return the same content as a single valid JSON object. No markdown. No prose."}).to_string();
            let repaired_raw = call_chat_model_raw(state, role, "You repair invalid JSON. Return JSON only.", &repair_user, Some(schema)).await?;
            let repaired = extract_model_content(&repaired_raw).unwrap_or_else(|| repaired_raw.to_string());
            let parsed = parse_json_object_from_text(&repaired).map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "model.invalid_json", format!("Model did not return valid JSON after repair: {error}"), "Use a stronger model, reduce temperature, or inspect vLLM output.").with_details(json!({"first_error": first_error, "raw": raw, "repair_raw": repaired_raw})))?;
            validate_model_json(schema, &parsed)?;
            Ok(parsed)
        }
    }
}

async fn call_chat_model_raw(state: &AppState, role: &ModelRole, system: &str, user: &str, schema: Option<SchemaKey>) -> Result<Value, ApiFailure> {
    state.provider_router
        .chat_raw(role, system, user, schema)
        .await
        .map_err(ApiFailure::from_provider_error)
}

async fn automatic_shacl_gate(state: &AppState, workspace: &Path, plan: &Value) -> Result<Value, ApiFailure> {
    run_shacl_gate(state, workspace, plan, "pre_codegen_shacl").await
}

async fn run_shacl_gate(state: &AppState, workspace: &Path, plan: &Value, stage: &str) -> Result<Value, ApiFailure> {
    let (data_ttl, graph_context) = collect_shacl_data_ttl(state, workspace, plan).await;
    let shapes_ttl = default_shacl_shapes();
    let slug = safe_iri_segment(stage);
    let data_path = workspace.join(format!("{slug}_shacl_data.ttl"));
    let shapes_path = workspace.join(format!("{slug}_shacl_shapes.ttl"));
    let report_path = workspace.join(if stage == "pre_codegen_shacl" { "automatic_shacl_report.json".to_string() } else { format!("{slug}_report.json") });
    let findings_path = workspace.join(if stage == "pre_codegen_shacl" { "automatic_shacl_findings.ttl".to_string() } else { format!("{slug}_findings.ttl") });
    let _ = fs::write(&data_path, &data_ttl).await;
    let _ = fs::write(&shapes_path, &shapes_ttl).await;

    if state.governance_mode.disabled() {
        let report = json!({
            "ok": true,
            "stage": stage,
            "governance_mode": state.governance_mode.as_str(),
            "enforced": false,
            "disabled": true,
            "conforms": true,
            "report_path": report_path.display().to_string(),
            "data_path": data_path.display().to_string(),
            "shapes_path": shapes_path.display().to_string(),
            "findings_path": findings_path.display().to_string(),
            "graph_context": graph_context,
            "message": "SHACL governance is disabled for this run. This mode cannot produce production_validated status."
        });
        let _ = fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string())).await;
        return Ok(report);
    }

    let result = run_shacl_validation(state, &data_ttl, &shapes_ttl).await;
    let report = match result {
        Ok(value) => {
            let conforms = value.get("conforms").and_then(Value::as_bool).unwrap_or(true);
            let ok = conforms;
            json!({
                "ok": ok,
                "stage": stage,
                "governance_mode": state.governance_mode.as_str(),
                "enforced": state.governance_mode.enforces(),
                "conforms": conforms,
                "blocking": state.governance_mode.enforces() && !ok,
                "status": if ok { "shacl_conforms" } else if state.governance_mode.enforces() { "blocked_by_governance" } else { "advisory_shacl_nonconformance" },
                "report_path": report_path.display().to_string(),
                "data_path": data_path.display().to_string(),
                "shapes_path": shapes_path.display().to_string(),
                "findings_path": findings_path.display().to_string(),
                "graph_context": graph_context,
                "result": value
            })
        }
        Err(error) => json!({
            "ok": false,
            "stage": stage,
            "governance_mode": state.governance_mode.as_str(),
            "enforced": state.governance_mode.enforces(),
            "conforms": false,
            "blocking": state.governance_mode.enforces(),
            "status": if state.governance_mode.enforces() { "blocked_by_governance" } else { "advisory_shacl_unavailable" },
            "report_path": report_path.display().to_string(),
            "data_path": data_path.display().to_string(),
            "shapes_path": shapes_path.display().to_string(),
            "findings_path": findings_path.display().to_string(),
            "graph_context": graph_context,
            "error": {"code": error.code, "message": error.message, "remediation": error.remediation}
        })
    };
    let findings_ttl = shacl_findings_to_turtle(workspace.file_name().and_then(|v| v.to_str()).unwrap_or("automatic"), &report);
    let _ = fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string())).await;
    let _ = fs::write(&findings_path, &findings_ttl).await;
    Ok(report)
}

fn plan_to_turtle(plan: &Value) -> String {
    let mut ttl = String::from("@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
    ttl.push_str("<urn:veritas:plan:auto> a veritas:EngineeringPlan, veritas:Plan ; veritas:hasTaskSpecification <urn:veritas:task:auto> ; veritas:hasAcceptanceCriterion <urn:veritas:acceptance:auto> ; veritas:validatedBy <urn:veritas:validation:auto> .\n");
    ttl.push_str("<urn:veritas:task:auto> a veritas:TaskSpecification .\n<urn:veritas:acceptance:auto> a veritas:AcceptanceCriterion .\n<urn:veritas:validation:auto> a veritas:ValidationCheckSpecification .\n");
    if let Some(risks) = plan.get("risks").and_then(Value::as_array) {
        for (i, _risk) in risks.iter().enumerate() {
            ttl.push_str(&format!("<urn:veritas:risk:auto:{i}> a veritas:Risk ; veritas:mitigatedBy <urn:veritas:mitigation:auto:{i}> .\n<urn:veritas:mitigation:auto:{i}> a veritas:MitigationSpecification .\n"));
        }
    }
    if let Some(symbolic_shadows) = plan.get("symbolic_shadows").and_then(Value::as_array) {
        for (i, shadow) in symbolic_shadows.iter().enumerate() {
            let expr = turtle_escape(shadow.get("expression_text").or_else(|| shadow.get("latex")).and_then(Value::as_str).unwrap_or("symbolic shadow"));
            let status = turtle_escape(shadow.get("human_validation_status").and_then(Value::as_str).unwrap_or("pending_human_review"));
            ttl.push_str(&format!("<urn:veritas:formula:auto:{i}> a veritas:SymbolicShadow ; veritas:derivedFrom <urn:veritas:evidence:auto:{i}> ; veritas:hasExpressionText \"{expr}\" ; veritas:hasFormulaSource \"planner_context\" ; veritas:hasHumanValidationStatus \"{status}\" ; veritas:hasConfidenceValue \"1.0\"^^xsd:decimal ; veritas:hasFormulaImageStatus \"not_applicable\" ; veritas:hasLatexOcrStatus \"not_required\" .\n<urn:veritas:evidence:auto:{i}> a veritas:EvidenceArtifact .\n"));
        }
    }
    if let Some(math) = plan.get("math_readiness") {
        ttl.push_str("<urn:veritas:math:auto> a veritas:MathematicalDiscoveryArtifact ; veritas:hasAxiomMap <urn:veritas:axiom-map:auto> ; veritas:hasRepresentationMap <urn:veritas:representation:auto> ; veritas:hasInvariant <urn:veritas:invariant:auto> ; veritas:hasValidationRequirement <urn:veritas:validation:auto> .\n");
        ttl.push_str("<urn:veritas:axiom-map:auto> a veritas:AxiomMap .\n<urn:veritas:representation:auto> a veritas:RepresentationMap ; veritas:mapsFromSurface <urn:veritas:surface:auto> ; veritas:mapsToLatentStructure <urn:veritas:latent:auto> ; veritas:preservesInvariant <urn:veritas:invariant:auto> .\n<urn:veritas:surface:auto> a veritas:SurfacePhenomenonDescription .\n<urn:veritas:latent:auto> a veritas:LatentStructureDescription .\n<urn:veritas:invariant:auto> a veritas:Invariant ; veritas:hasTransformationFamily <urn:veritas:transformation-family:auto> .\n<urn:veritas:transformation-family:auto> a veritas:TransformationFamily .\n");
        if math.get("necessity_claim").is_some() {
            ttl.push_str("<urn:veritas:necessity:auto> a veritas:GenerativeNecessityClaim ; veritas:supportedByEvidence <urn:veritas:evidence:auto:necessity> ; veritas:hasProofStatus \"speculative\" ; veritas:testedByTransferTest <urn:veritas:transfer:auto> .\n<urn:veritas:evidence:auto:necessity> a veritas:EvidenceArtifact .\n<urn:veritas:transfer:auto> a veritas:TransferTestSpecification .\n");
        }
    }
    ttl
}

fn default_shacl_shapes() -> String {
    format!("{}\n\n{}",
        include_str!("../../../packages/ontology/shacl/veritas-core.shacl.ttl"),
        include_str!("../../../packages/ontology/shacl/veritas-math.shacl.ttl")
    )
}

fn shacl_graph_context_construct_query() -> &'static str {
    r#"
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
CONSTRUCT {
  ?s ?p ?o .
}
WHERE {
  GRAPH ?g {
    ?s ?p ?o .
    FILTER (?g != <urn:veritas:graph:ontology>)
    FILTER EXISTS { ?s a ?type . FILTER (?type IN (veritas:SymbolicShadow, veritas:MathematicalDiscoveryArtifact, veritas:RepresentationMap, veritas:Invariant, veritas:GenerativeNecessityClaim, veritas:SourceCodeArtifact, veritas:BuildArtifact, veritas:Risk, veritas:Plan, veritas:LoopSpecification)) }
  }
}
LIMIT 1000
"#
}

async fn fetch_shacl_graph_context_ttl(state: &AppState) -> Result<String, ApiFailure> {
    let response = state.http.post(&state.fuseki_query_url)
        .header("accept", "text/turtle")
        .form(&[("query", shacl_graph_context_construct_query().to_string())])
        .send()
        .await
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "shacl.context.transport", format!("Fuseki SHACL context query failed before response: {error}"), "Check Fuseki readiness and graph-store configuration."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "shacl.context.upstream", format!("Fuseki returned HTTP {} while collecting SHACL context", status.as_u16()), "Check Fuseki logs and whether graphs have been loaded.").with_details(json!({"raw": text})));
    }
    Ok(text)
}

fn configured_shacl_artifact_files() -> Vec<String> {
    let configured = env::var("VERITAS_SHACL_ARTIFACT_FILES").unwrap_or_default();
    if configured.trim().is_empty() {
        return [
            "evidence_manifest.json",
            "formula_manifest.json",
            "citation_manifest.json",
            "evidence_registry.json",
            "evidence_eligibility.json",
            "review_queue.json",
            "representation_model.json",
            "planning_context.json",
            "plan.json",
            "code_package_latest.json",
            "validation_results.json",
            "human_checkpoints.jsonl",
            "math_validation_report.json",
            "math_tool_results.jsonl",
            "gate_decisions.jsonl",
        ].iter().map(|item| item.to_string()).collect();
    }
    configured
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

async fn collect_artifact_bundle_ttl(workspace: &Path, plan: &Value) -> (String, Value) {
    let run = safe_iri_segment(workspace.file_name().and_then(|v| v.to_str()).unwrap_or("run"));
    let mut ttl = String::from("@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n@prefix dcterms: <http://purl.org/dc/terms/> .\n");
    ttl.push_str(&format!("<urn:veritas:run:{run}> a veritas:PlannedEngineeringAct ; veritas:hasIdentifier \"{}\" .\n", turtle_escape(&run)));
    ttl.push_str("\n# Planner-generated plan obligations.\n");
    ttl.push_str(&plan_to_turtle(plan));
    ttl.push_str("\n# Workspace artifact bundle obligations.\n");

    let mut artifacts = Vec::new();
    for file_name in configured_shacl_artifact_files() {
        let path = workspace.join(&file_name);
        if !path.exists() {
            artifacts.push(json!({"path": file_name, "present": false}));
            continue;
        }
        let content = match fs::read_to_string(&path).await {
            Ok(text) => text,
            Err(error) => {
                artifacts.push(json!({"path": file_name, "present": true, "readable": false, "error": error.to_string()}));
                continue;
            }
        };
        let artifact_id = safe_iri_segment(&file_name);
        ttl.push_str(&format!("<urn:veritas:artifact:{run}:{artifact_id}> a veritas:EvidenceArtifact ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:hasIdentifier \"{}\" ; veritas:hasStatus \"present\" .\n", turtle_escape(&file_name)));
        let mut parsed_records = 0usize;
        if file_name.ends_with(".jsonl") {
            for (idx, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() { continue; }
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    parsed_records += 1;
                    append_known_artifact_value_ttl(&mut ttl, &run, &file_name, &value, idx, workspace);
                }
            }
        } else if let Ok(value) = serde_json::from_str::<Value>(&content) {
            parsed_records = 1;
            append_known_artifact_value_ttl(&mut ttl, &run, &file_name, &value, 0, workspace);
        }
        artifacts.push(json!({"path": file_name, "present": true, "readable": true, "bytes": content.len(), "records": parsed_records}));
    }
    (ttl, json!({"ok": true, "source": "workspace_artifact_bundle", "files": artifacts}))
}

fn append_known_artifact_value_ttl(ttl: &mut String, run: &str, file_name: &str, value: &Value, record_idx: usize, workspace: &Path) {
    match file_name {
        name if name.contains("formula") || name == "evidence_registry.json" || name == "evidence_eligibility.json" => append_formula_records_ttl(ttl, run, file_name, value),
        name if name.contains("citation") => append_citation_records_ttl(ttl, run, file_name, value),
        "evidence_manifest.json" => append_evidence_manifest_ttl(ttl, run, value),
        "representation_model.json" => append_representation_model_ttl(ttl, run, value),
        "planning_context.json" => append_planning_context_ttl(ttl, run, value),
        "code_package_latest.json" => append_code_package_ttl(ttl, run, value, workspace),
        "validation_results.json" => append_validation_results_ttl(ttl, run, value),
        "math_validation_report.json" => append_math_validation_report_ttl(ttl, run, value),
        "human_checkpoints.jsonl" | "gate_decisions.jsonl" | "math_tool_results.jsonl" => append_jsonl_record_ttl(ttl, run, file_name, value, record_idx),
        _ => {}
    }
}

fn collect_json_records(value: &Value, keys: &[&str]) -> Vec<Value> {
    if let Some(items) = value.as_array() {
        return items.clone();
    }
    for key in keys {
        if let Some(items) = value.get(*key).and_then(Value::as_array) {
            return items.clone();
        }
    }
    Vec::new()
}

fn json_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            if !text.trim().is_empty() { return Some(text); }
        }
    }
    None
}

fn json_f64_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(num) = value.get(*key).and_then(Value::as_f64) {
            return Some(format!("{num}"));
        }
        if let Some(text) = value.get(*key).and_then(Value::as_str) {
            if text.parse::<f64>().is_ok() { return Some(text.to_string()); }
        }
    }
    None
}

fn append_formula_records_ttl(ttl: &mut String, run: &str, file_name: &str, value: &Value) {
    let mut records = collect_json_records(value, &["records", "formulas", "eligible_formulas", "blocked_formulas"]);
    if records.is_empty() && value.get("formula_id").is_some() { records.push(value.clone()); }
    for (idx, record) in records.iter().enumerate() {
        let formula_id = json_str(record, &["formula_id", "id"]).map(ToString::to_string).unwrap_or_else(|| format!("{file_name}-{idx}"));
        let fid = safe_iri_segment(&formula_id);
        let evidence_id = json_str(record, &["citation_id", "source_document_id", "paper_id", "chunk_id"]).unwrap_or(&formula_id);
        let eid = safe_iri_segment(evidence_id);
        ttl.push_str(&format!("<urn:veritas:evidence:{run}:{eid}> a veritas:EvidenceArtifact ; veritas:hasIdentifier \"{}\" .\n", turtle_escape(evidence_id)));
        ttl.push_str(&format!("<urn:veritas:formula:{run}:{fid}> a veritas:SymbolicShadow ; veritas:derivedFrom <urn:veritas:evidence:{run}:{eid}>"));
        if let Some(expr) = json_str(record, &["normalized_latex", "normalized_expression_text", "latex", "raw_latex", "expression_text"]) {
            ttl.push_str(&format!(" ; veritas:hasExpressionText \"{}\"", turtle_escape(expr)));
            ttl.push_str(&format!(" ; veritas:hasNormalizedExpressionText \"{}\"", turtle_escape(expr)));
        }
        if let Some(source) = json_str(record, &["formula_source", "source", "extraction_source", "ocr_engine", "formula_image_engine"]) {
            ttl.push_str(&format!(" ; veritas:hasFormulaSource \"{}\"", turtle_escape(source)));
        }
        if let Some(status) = json_str(record, &["human_validation_status", "review_decision", "codegen_eligibility_status", "normalized_codegen_status"]) {
            ttl.push_str(&format!(" ; veritas:hasHumanValidationStatus \"{}\"", turtle_escape(status)));
            ttl.push_str(&format!(" ; veritas:hasCodegenEligibilityStatus \"{}\"", turtle_escape(status)));
        }
        if let Some(confidence) = json_f64_string(record, &["confidence", "ocr_confidence", "latex_ocr_confidence", "formula_image_confidence"]) {
            ttl.push_str(&format!(" ; veritas:hasConfidenceValue \"{}\"^^xsd:decimal", turtle_escape(&confidence)));
        }
        if let Some(image_status) = json_str(record, &["formula_image_status", "image_status"]) {
            ttl.push_str(&format!(" ; veritas:hasFormulaImageStatus \"{}\"", turtle_escape(image_status)));
        }
        if let Some(ocr_status) = json_str(record, &["latex_ocr_status", "ocr_status"]) {
            ttl.push_str(&format!(" ; veritas:hasLatexOcrStatus \"{}\"", turtle_escape(ocr_status)));
        }
        ttl.push_str(" .\n");
    }
}

fn append_citation_records_ttl(ttl: &mut String, run: &str, file_name: &str, value: &Value) {
    let mut records = collect_json_records(value, &["records", "citations", "approved_citations", "blocked_citations"]);
    if records.is_empty() && value.get("citation_id").is_some() { records.push(value.clone()); }
    for (idx, record) in records.iter().enumerate() {
        let citation_id = json_str(record, &["citation_id", "source_document_id", "paper_id", "id"]).map(ToString::to_string).unwrap_or_else(|| format!("{file_name}-{idx}"));
        let cid = safe_iri_segment(&citation_id);
        ttl.push_str(&format!("<urn:veritas:citation:{run}:{cid}> a veritas:EvidenceArtifact ; veritas:hasIdentifier \"{}\"", turtle_escape(&citation_id)));
        if let Some(status) = json_str(record, &["normalized_review_status", "citation_review_status", "review_decision"]) {
            ttl.push_str(&format!(" ; veritas:hasStatus \"{}\"", turtle_escape(status)));
        }
        if let Some(apa) = json_str(record, &["apa_citation", "citation", "title"]) {
            ttl.push_str(&format!(" ; veritas:hasDescription \"{}\"", turtle_escape(apa)));
        }
        ttl.push_str(" .\n");
    }
}

fn append_evidence_manifest_ttl(ttl: &mut String, run: &str, value: &Value) {
    let source_id = json_str(value, &["source_document_id", "paper_id", "document_id"]).unwrap_or("source-document");
    let sid = safe_iri_segment(source_id);
    ttl.push_str(&format!("<urn:veritas:document:{run}:{sid}> a veritas:EvidenceArtifact ; veritas:hasIdentifier \"{}\"", turtle_escape(source_id)));
    if let Some(status) = json_str(value, &["planning_status", "status"]) { ttl.push_str(&format!(" ; veritas:hasStatus \"{}\"", turtle_escape(status))); }
    if let Some(path) = json_str(value, &["source_pdf", "path"]) { ttl.push_str(&format!(" ; veritas:hasDescription \"{}\"", turtle_escape(path))); }
    ttl.push_str(" .\n");
}

fn append_representation_model_ttl(ttl: &mut String, run: &str, value: &Value) {
    let status = json_str(value, &["status", "review_status"]).unwrap_or("pending_review");
    ttl.push_str(&format!("<urn:veritas:math-model:{run}> a veritas:MathematicalDiscoveryArtifact ; veritas:hasStatus \"{}\"", turtle_escape(status)));
    if value.get("axiom_map").is_some() || value.get("axioms").is_some() { ttl.push_str(&format!(" ; veritas:hasAxiomMap <urn:veritas:axiom-map:{run}>")); }
    if value.get("representation_map").is_some() { ttl.push_str(&format!(" ; veritas:hasRepresentationMap <urn:veritas:representation:{run}>")); }
    if value.get("invariants").is_some() || value.get("invariant").is_some() { ttl.push_str(&format!(" ; veritas:hasInvariant <urn:veritas:invariant:{run}>")); }
    if value.get("validation_obligations").is_some() || value.get("validation_requirements").is_some() { ttl.push_str(&format!(" ; veritas:hasValidationRequirement <urn:veritas:validation:{run}>")); }
    ttl.push_str(" .\n");
    if value.get("axiom_map").is_some() || value.get("axioms").is_some() { ttl.push_str(&format!("<urn:veritas:axiom-map:{run}> a veritas:AxiomMap .\n")); }
    if value.get("representation_map").is_some() {
        ttl.push_str(&format!("<urn:veritas:representation:{run}> a veritas:RepresentationMap ; veritas:mapsFromSurface <urn:veritas:surface:{run}> ; veritas:mapsToLatentStructure <urn:veritas:latent:{run}>"));
        if value.get("invariants").is_some() || value.get("invariant").is_some() { ttl.push_str(&format!(" ; veritas:preservesInvariant <urn:veritas:invariant:{run}>")); }
        ttl.push_str(" .\n");
        ttl.push_str(&format!("<urn:veritas:surface:{run}> a veritas:SurfacePhenomenonDescription .\n<urn:veritas:latent:{run}> a veritas:LatentStructureDescription .\n"));
    }
    if value.get("invariants").is_some() || value.get("invariant").is_some() {
        ttl.push_str(&format!("<urn:veritas:invariant:{run}> a veritas:Invariant ; veritas:hasTransformationFamily <urn:veritas:transformation-family:{run}> .\n<urn:veritas:transformation-family:{run}> a veritas:TransformationFamily .\n"));
    }
    if value.get("validation_obligations").is_some() || value.get("validation_requirements").is_some() { ttl.push_str(&format!("<urn:veritas:validation:{run}> a veritas:ValidationCheckSpecification .\n")); }
}

fn append_planning_context_ttl(ttl: &mut String, run: &str, value: &Value) {
    let records = collect_json_records(value, &["retrieved_evidence", "approved_citations", "eligible_formulas", "ontology_facts"]);
    for (idx, record) in records.iter().enumerate() {
        let id = json_str(record, &["id", "evidence_id", "citation_id", "formula_id"]).map(ToString::to_string).unwrap_or_else(|| format!("planning-evidence-{idx}"));
        let eid = safe_iri_segment(&id);
        ttl.push_str(&format!("<urn:veritas:planning-context:{run}:{eid}> a veritas:EvidenceArtifact ; veritas:hasIdentifier \"{}\" ; veritas:derivedFrom <urn:veritas:run:{run}> .\n", turtle_escape(&id)));
    }
}

fn append_code_package_ttl(ttl: &mut String, run: &str, value: &Value, workspace: &Path) {
    let has_validation = workspace.join("validation_results.json").exists();
    let files = collect_json_records(value, &["files", "source_files"]);
    for (idx, file) in files.iter().enumerate() {
        let path = json_str(file, &["path", "file_path", "name"]).map(ToString::to_string).unwrap_or_else(|| format!("generated-file-{idx}"));
        let fid = safe_iri_segment(&path);
        ttl.push_str(&format!("<urn:veritas:source:{run}:{fid}> a veritas:SourceCodeArtifact ; veritas:hasIdentifier \"{}\" ; veritas:derivedFrom <urn:veritas:run:{run}>", turtle_escape(&path)));
        if has_validation { ttl.push_str(&format!(" ; veritas:validatedBy <urn:veritas:validation:{run}> ; veritas:testedBy <urn:veritas:test:{run}:{idx}>")); }
        ttl.push_str(" .\n");
        if has_validation { ttl.push_str(&format!("<urn:veritas:test:{run}:{idx}> a veritas:TestSpecification .\n")); }
    }
}

fn append_validation_results_ttl(ttl: &mut String, run: &str, value: &Value) {
    let passed = value.to_string().contains("\"success\":true") || value.to_string().contains("\"success\": true");
    let status = if passed { "passed" } else { "failed_or_pending" };
    ttl.push_str(&format!("<urn:veritas:validation:{run}> a veritas:ValidationCheckSpecification, veritas:VerificationResult ; veritas:hasStatus \"{}\" ; veritas:derivedFrom <urn:veritas:run:{run}> .\n", status));
    if passed { ttl.push_str(&format!("<urn:veritas:build:{run}> a veritas:BuildArtifact ; veritas:hasStatus \"local_validated\" ; veritas:validatedBy <urn:veritas:validation:{run}> ; veritas:derivedFrom <urn:veritas:run:{run}> .\n")); }
}

fn append_math_validation_report_ttl(ttl: &mut String, run: &str, value: &Value) {
    let status = json_str(value, &["status"]).unwrap_or_else(|| if value.get("ok").and_then(Value::as_bool).unwrap_or(false) { "passed" } else { "blocked_by_math_tools" });
    ttl.push_str(&format!("<urn:veritas:math-validation:{run}> a veritas:ValidationCheckSpecification ; veritas:hasStatus \"{}\" ; veritas:derivedFrom <urn:veritas:run:{run}> .\n", turtle_escape(status)));
}

fn append_jsonl_record_ttl(ttl: &mut String, run: &str, file_name: &str, value: &Value, idx: usize) {
    let id = safe_iri_segment(&format!("{}-{}", file_name, idx));
    let status = json_str(value, &["status", "decision", "human_decision", "final_status"]).unwrap_or("recorded");
    ttl.push_str(&format!("<urn:veritas:event:{run}:{id}> a veritas:EvidenceArtifact ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:hasIdentifier \"{}\" ; veritas:hasStatus \"{}\" .\n", turtle_escape(file_name), turtle_escape(status)));
}

async fn collect_shacl_data_ttl(state: &AppState, workspace: &Path, plan: &Value) -> (String, Value) {
    let (artifact_ttl, artifact_context) = collect_artifact_bundle_ttl(workspace, plan).await;
    let mut data_ttl = artifact_ttl;
    match fetch_shacl_graph_context_ttl(state).await {
        Ok(context_ttl) if !context_ttl.trim().is_empty() => {
            data_ttl.push_str("\n# Fuseki graph-derived SHACL context.\n");
            data_ttl.push_str(&context_ttl);
            (data_ttl, json!({"ok": true, "source": "artifact_bundle_plus_fuseki_construct", "artifact_bundle": artifact_context, "fuseki": {"ok": true, "bytes": context_ttl.len()}}))
        }
        Ok(_) => (data_ttl, json!({"ok": true, "source": "artifact_bundle", "artifact_bundle": artifact_context, "fuseki": {"ok": true, "bytes": 0, "warning": "No graph-derived SHACL context returned."}})),
        Err(error) => {
            data_ttl.push_str("\n# Fuseki graph-derived SHACL context unavailable; validating artifact bundle only.\n");
            (data_ttl, json!({"ok": artifact_context.get("ok").and_then(Value::as_bool).unwrap_or(true), "source": "artifact_bundle", "artifact_bundle": artifact_context, "fuseki": {"ok": false, "code": error.code, "message": error.message, "remediation": error.remediation}}))
        }
    }
}

fn shacl_findings_to_turtle(run_id: &str, report: &Value) -> String {
    let mut ttl = String::from("@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
    let run = safe_iri_segment(run_id);
    let conforms = report.get("conforms").and_then(Value::as_bool).unwrap_or(true);
    if conforms {
        ttl.push_str(&format!("<urn:veritas:shacl-finding:{run}:conforms> a veritas:Finding ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:hasStatus \"closed\" ; veritas:hasDescription \"SHACL validation conformed.\" .\n"));
        return ttl;
    }
    let text = report.get("results_text").and_then(Value::as_str).unwrap_or("SHACL validation failed.");
    for (idx, line) in text.lines().filter(|line| line.contains("Message:") || line.contains("Focus Node:") || line.contains("Result Path:")).take(50).enumerate() {
        ttl.push_str(&format!("<urn:veritas:shacl-finding:{run}:{idx}> a veritas:Finding ; veritas:derivedFrom <urn:veritas:run:{run}> ; veritas:hasStatus \"open\" ; veritas:hasDescription \"{}\" .\n", turtle_escape(line.trim())));
    }
    ttl
}

pub(crate) fn shacl_report_conforms(report: &Value) -> bool {
    let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let conforms = report.get("conforms").and_then(Value::as_bool)
        .or_else(|| report.pointer("/result/conforms").and_then(Value::as_bool))
        .unwrap_or(ok);
    ok && conforms
}

async fn run_shacl_validation(state: &AppState, data_ttl: &str, shapes_ttl: &str) -> Result<Value, ApiFailure> {
    let url = format!("{}/validate", state.shacl_url.trim_end_matches('/'));
    let response = state.http.post(url).json(&json!({"data_ttl": data_ttl, "shapes_ttl": if shapes_ttl.trim().is_empty() { Value::Null } else { json!(shapes_ttl) }})).send().await
        .map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "shacl.transport", format!("SHACL validator request failed: {error}"), "Start the shacl service and inspect `docker compose logs shacl`."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "shacl.upstream", format!("SHACL validator returned HTTP {}", status.as_u16()), "Check SHACL service and rule pack.").with_details(body));
    }
    Ok(body)
}



async fn upload_run_report_to_fuseki(state: &AppState, run_id: &str, report: &Value) -> Result<Value, ApiFailure> {
    let graph_uri = format!("{}:{}", state.graph_run_base_uri, safe_iri_segment(run_id));
    let validation_graph_uri = format!("{}:{}", state.graph_validation_base_uri, safe_iri_segment(run_id));
    let status = report.get("final_status").and_then(Value::as_str).unwrap_or("unknown");
    let goal = report.get("original_task").and_then(Value::as_str).unwrap_or("");
    let mut ttl = String::from("@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n@prefix dcterms: <http://purl.org/dc/terms/> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
    ttl.push_str(&format!("<urn:veritas:run:{}> a veritas:PlannedEngineeringAct ; veritas:hasIdentifier \"{}\" ; veritas:hasStatus \"{}\" ; veritas:hasDescription \"{}\" .\n", safe_iri_segment(run_id), turtle_escape(run_id), turtle_escape(status), turtle_escape(goal)));
    if let Some(files) = report.get("files_changed").and_then(Value::as_array) {
        for (idx, file) in files.iter().enumerate() {
            let path = file.as_str().unwrap_or("");
            if path.is_empty() { continue; }
            ttl.push_str(&format!("<urn:veritas:source:{}:{}> a veritas:SourceCodeArtifact ; veritas:hasIdentifier \"{}\" ; veritas:derivedFrom <urn:veritas:run:{}> ; veritas:validatedBy <urn:veritas:validation:{}> ; veritas:testedBy <urn:veritas:test:{}:{}> .\n<urn:veritas:test:{}:{}> a veritas:TestSpecification .\n", safe_iri_segment(run_id), idx, turtle_escape(path), safe_iri_segment(run_id), safe_iri_segment(run_id), safe_iri_segment(run_id), idx, safe_iri_segment(run_id), idx));
        }
    }
    if status == "production_candidate_validated" {
        ttl.push_str(&format!("<urn:veritas:build:{}> a veritas:BuildArtifact ; veritas:hasStatus \"production_candidate_validated\" ; veritas:validatedBy <urn:veritas:validation:{}> ; veritas:derivedFrom <urn:veritas:run:{}> .\n", safe_iri_segment(run_id), safe_iri_segment(run_id), safe_iri_segment(run_id)));
    }
    let run_upload = upload_turtle_to_fuseki(state, &graph_uri, &ttl, true, "text/turtle").await?;
    let mut validation_ttl = format!("@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .\n<urn:veritas:validation:{}> a veritas:VerificationResult ; veritas:hasStatus \"{}\" ; veritas:derivedFrom <urn:veritas:run:{}> .\n", safe_iri_segment(run_id), turtle_escape(status), safe_iri_segment(run_id));
    if let Some(shacl_report) = report.get("automatic_shacl") {
        validation_ttl.push_str(&shacl_findings_to_turtle(run_id, shacl_report));
    }
    if let Some(shacl_report) = report.get("final_shacl") {
        validation_ttl.push_str(&shacl_findings_to_turtle(run_id, shacl_report));
    }
    let validation_upload = upload_turtle_to_fuseki(state, &validation_graph_uri, &validation_ttl, true, "text/turtle").await?;
    Ok(json!({"ok": true, "run_graph_uri": graph_uri, "validation_graph_uri": validation_graph_uri, "run_upload": run_upload, "validation_upload": validation_upload}))
}

fn safe_iri_segment(value: &str) -> String {
    let cleaned: String = value.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() { "item".into() } else { trimmed.to_string() }
}

fn turtle_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ").replace('\r', " ")
}

pub(crate) async fn write_json_file(path: &Path, value: &Value) -> Result<(), ApiFailure> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "json.mkdir", format!("Could not create directory for {}: {error}", path.display()), "Check workspace permissions."))?;
    }
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "json.write_tmp", format!("Could not write temporary JSON file {}: {error}", tmp.display()), "Check workspace permissions."))?;
    fs::rename(&tmp, path).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "json.atomic_rename", format!("Could not atomically replace {}: {error}", path.display()), "Check workspace permissions and filesystem semantics."))?;
    Ok(())
}

async fn acquire_run_lock(workspace: &Path, run_id: &str) -> Result<RunLock, ApiFailure> {
    fs::create_dir_all(workspace).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.lock.mkdir", format!("Could not create run workspace before locking: {error}"), "Check VERITAS_RUNS_DIR permissions."))?;
    let lock_path = workspace.join("run.lock");
    if lock_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&lock_path) {
            if let Ok(modified) = metadata.modified() {
                let stale_after = uint_env("VERITAS_RUN_LOCK_STALE_SECS", 7200) as u64;
                if modified.elapsed().map(|age| age.as_secs() > stale_after).unwrap_or(false) {
                    let _ = std::fs::remove_file(&lock_path);
                }
            }
        }
    }
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&lock_path).await
        .map_err(|error| {
            let remediation = "Another Veritas worker is already advancing this run, or a stale run.lock exists. Inspect /status/:run_id; set VERITAS_RUN_LOCK_STALE_SECS or remove the lock only after confirming no worker is active.";
            ApiFailure::new(StatusCode::CONFLICT, "run.locked", format!("Could not acquire run lock for {run_id}: {error}"), remediation)
                .with_details(json!({"lock_path": lock_path.display().to_string()}))
        })?;
    let payload = json!({"run_id": run_id, "pid": std::process::id(), "created_at_ms": now_millis()});
    let mut line = payload.to_string();
    line.push('\n');
    file.write_all(line.as_bytes()).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.lock.write", format!("Could not write run lock metadata: {error}"), "Check workspace permissions."))?;
    Ok(RunLock { path: lock_path, run_id: run_id.to_string() })
}

async fn append_command_audit(workspace: &Path, result: &Value) -> Result<(), ApiFailure> {
    use tokio::io::AsyncWriteExt;
    let path = workspace.join("command_audit.jsonl");
    let mut event = result.clone();
    event["ts_ms"] = json!(now_millis());
    let mut line = serde_json::to_string(&event).unwrap_or_else(|_| event.to_string());
    line.push('\n');
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(&path).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "command.audit_open", format!("Could not open command audit log: {error}"), "Check workspace permissions."))?;
    file.write_all(line.as_bytes()).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "command.audit_write", format!("Could not write command audit log: {error}"), "Check workspace permissions."))?;
    Ok(())
}

pub(crate) async fn append_jsonl(path: &Path, value: &Value) -> Result<(), ApiFailure> {
    use tokio::io::AsyncWriteExt;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "jsonl.mkdir", format!("Could not create directory for {}: {error}", path.display()), "Check workspace permissions."))?;
    }
    let mut line = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    line.push('\n');
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "jsonl.open", format!("Could not open {}: {error}", path.display()), "Check workspace permissions."))?;
    file.write_all(line.as_bytes()).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "jsonl.write", format!("Could not write {}: {error}", path.display()), "Check workspace permissions."))?;
    Ok(())
}

pub(crate) async fn persist_run_state(workspace: &Path, state_name: &str, payload: Value) -> Result<(), ApiFailure> {
    let state_file = workspace.join("state.json");
    let event_file = workspace.join("events.jsonl");
    let sequence = next_event_sequence(&event_file).await;
    let event = json!({"ts_ms": now_millis(), "sequence": sequence, "state": state_name, "payload": payload});
    write_json_file(&state_file, &event).await?;
    let mut line = serde_json::to_string(&event).unwrap_or_else(|_| event.to_string());
    line.push('\n');
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(event_file).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.event_open", format!("Could not open events log: {error}"), "Check run workspace permissions."))?;
    file.write_all(line.as_bytes()).await
        .map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.event_write", format!("Could not append events log: {error}"), "Check run workspace permissions."))?;
    append_run_index_event(workspace, &event).await?;
    Ok(())
}

async fn append_run_index_event(workspace: &Path, event: &Value) -> Result<(), ApiFailure> {
    let runs_dir = workspace.parent().ok_or_else(|| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.index.parent_missing", "Run workspace has no parent runs directory.", "Check VERITAS_RUNS_DIR configuration."))?;
    let run_id = workspace.file_name().map(|v| v.to_string_lossy().to_string()).unwrap_or_else(|| "unknown".to_string());
    let index_event = json!({
        "run_id": run_id,
        "workspace": workspace.display().to_string(),
        "state": event.get("state").cloned().unwrap_or_else(|| json!(null)),
        "sequence": event.get("sequence").cloned().unwrap_or_else(|| json!(null)),
        "ts_ms": event.get("ts_ms").cloned().unwrap_or_else(|| json!(now_millis())),
        "payload": event.get("payload").cloned().unwrap_or_else(|| json!({}))
    });
    append_jsonl(&runs_dir.join("run_index.jsonl"), &index_event).await
}

async fn next_event_sequence(event_file: &Path) -> u64 {
    match fs::read_to_string(event_file).await {
        Ok(text) => text.lines().count() as u64,
        Err(_) => 0,
    }
}

fn command_allowed(command: &str) -> bool {
    command_rejection_reason(command).is_none()
}

fn command_rejection_reason(command: &str) -> Option<String> {
    let c = command.trim();
    if c.is_empty() {
        return Some("command is empty".to_string());
    }
    let denied_substrings = [
        ";", "&&", "||", "|", "`", "$(", "\n", "\r", ">", "<",
        "rm ", "rm -", "sudo", "curl ", "wget ", ":(){", "mkfs", "dd ",
        "chmod ", "chown ", "> /etc", "docker ", "podman ", "ssh ", "scp ", "nc ",
        "bash -c", "sh -c", "python -c", "python3 -c",
    ];
    if let Some(token) = denied_substrings.iter().find(|token| c.contains(**token)) {
        return Some(format!("command contains denied shell/system token: {token}"));
    }
    let allowed_prefixes = [
        "cargo fmt", "cargo check", "cargo test", "cargo clippy",
        "python -m pytest", "python3 -m pytest", "python -m build", "python3 -m build",
        "ruff", "mypy", "cmake", "ctest"
    ];
    if allowed_prefixes.iter().any(|prefix| c == *prefix || c.starts_with(&format!("{prefix} "))) {
        None
    } else {
        Some("command does not start with an approved compile/test/static-analysis tool".to_string())
    }
}

fn extract_model_content(raw: &Value) -> Option<String> {
    raw.pointer("/choices/0/message/content").and_then(Value::as_str).map(ToString::to_string)
        .or_else(|| raw.pointer("/choices/0/text").and_then(Value::as_str).map(ToString::to_string))
}

fn parse_json_object_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.is_object() { return Ok(value); }
    }
    let start = trimmed.find('{').ok_or_else(|| "no opening `{` found".to_string())?;
    let end = trimmed.rfind('}').ok_or_else(|| "no closing `}` found".to_string())?;
    let slice = &trimmed[start..=end];
    let value: Value = serde_json::from_str(slice).map_err(|error| error.to_string())?;
    if value.is_object() { Ok(value) } else { Err("parsed JSON is not an object".to_string()) }
}

fn compact_search_evidence(evidence: &Value) -> Value {
    let hits = evidence.pointer("/hits/hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let compact: Vec<Value> = hits.into_iter().take(8).map(|hit| {
        json!({
            "id": hit.get("_id"),
            "score": hit.get("_score"),
            "source": hit.get("_source").map(|src| json!({
                "paper_id": src.get("paper_id"),
                "title": src.get("title"),
                "chunk_id": src.get("chunk_id"),
                "text": src.get("text").and_then(Value::as_str).map(|s| s.chars().take(1600).collect::<String>()),
                "formulas": src.get("formulas")
            }))
        })
    }).collect();
    json!({"hits": compact})
}

pub(crate) fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}
