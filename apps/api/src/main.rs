use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{fs, process::Command, sync::Mutex, time::timeout};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone, Debug, Serialize)]
struct ModelRole {
    role: &'static str,
    url: String,
    model: String,
    served_model_name: String,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
    timeout_secs: u64,
}

#[derive(Clone)]
struct AppState {
    http: Client,
    opensearch_url: String,
    opensearch_index: String,
    opensearch_vector_field: String,
    fuseki_query_url: String,
    fuseki_ping_url: String,
    embedding_url: String,
    require_models: bool,
    planner_model: ModelRole,
    code_model: ModelRole,
    math_model: ModelRole,
    code_fallback_model: String,
    math_large_model: String,
    remote_model_enabled: bool,
    remote_model_base_url: String,
    remote_model_name: String,
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
struct EmbedResponse {
    vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
struct RunRequest {
    goal: String,
    language: Option<String>,
    size: Option<u32>,
    max_retries: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Health {
    service: &'static str,
    status: &'static str,
}

#[derive(Debug)]
struct ApiFailure {
    status: StatusCode,
    code: String,
    message: String,
    remediation: String,
    details: Value,
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
    let state = Arc::new(AppState {
        http: Client::new(),
        opensearch_url: env::var("VERITAS_OPENSEARCH_URL")
            .unwrap_or_else(|_| "http://opensearch:9200".into()),
        opensearch_index: env::var("VERITAS_OPENSEARCH_INDEX")
            .unwrap_or_else(|_| "veritas-papers".into()),
        opensearch_vector_field: env::var("VERITAS_OPENSEARCH_VECTOR_FIELD")
            .unwrap_or_else(|_| "embedding".into()),
        fuseki_query_url: env::var("VERITAS_FUSEKI_QUERY_URL")
            .unwrap_or_else(|_| "http://fuseki:3030/veritas/sparql".into()),
        fuseki_ping_url: env::var("VERITAS_FUSEKI_PING_URL")
            .unwrap_or_else(|_| "http://fuseki:3030/$/ping".into()),
        embedding_url: env::var("VERITAS_EMBEDDING_URL")
            .unwrap_or_else(|_| "http://embedding:8090".into()),
        require_models: bool_env("VERITAS_REQUIRE_MODELS", true),
        planner_model: model_role("planner", "VERITAS_PLANNER", "http://vllm-planner:8000", "Qwen/Qwen2.5-Coder-7B-Instruct", "veritas-planner", 0.05, 0.9, 2200),
        code_model: model_role("code_generation", "VERITAS_CODE", "http://vllm-code:8000", "Qwen/Qwen2.5-Coder-14B-Instruct", "veritas-code", 0.02, 0.9, 7000),
        math_model: model_role("math_reasoning", "VERITAS_MATH", "http://vllm-math:8000", "allenai/Olmo-3-7B-Instruct", "veritas-math", 0.05, 0.9, 5000),
        code_fallback_model: env::var("VERITAS_CODE_FALLBACK_MODEL")
            .unwrap_or_else(|_| "deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct".into()),
        math_large_model: env::var("VERITAS_MATH_LARGE_MODEL")
            .unwrap_or_else(|_| "allenai/Olmo-3.1-32B-Instruct".into()),
        remote_model_enabled: bool_env("VERITAS_REMOTE_MODEL_ENABLED", false),
        remote_model_base_url: env::var("VERITAS_REMOTE_MODEL_BASE_URL").unwrap_or_default(),
        remote_model_name: env::var("VERITAS_REMOTE_MODEL_NAME").unwrap_or_default(),
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
        .route("/graph/status", get(graph_status))
        .route("/sparql", post(sparql))
        .route("/search", post(search))
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
    let planner = probe_model(&state.http, &state.planner_model).await;
    let code = probe_model(&state.http, &state.code_model).await;
    let math = probe_model(&state.http, &state.math_model).await;
    let base_ok = opensearch["ok"].as_bool().unwrap_or(false)
        && fuseki["ok"].as_bool().unwrap_or(false)
        && embedding["ok"].as_bool().unwrap_or(false);
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
                "vllm_planner": planner,
                "vllm_code": code,
                "vllm_math": math
            },
            "help": if ok { "Required services are reachable." } else { "Run `docker compose ps` and `docker compose logs --tail=200`; for local models run `docker compose --profile models --profile code-model --profile math-model up -d`." }
        })),
    )
}

async fn models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "serving_solution": "vllm",
            "protocol": "OpenAI-compatible /v1/chat/completions",
            "planner": role_json(&state.planner_model),
            "code_generation": {
                "primary": role_json(&state.code_model),
                "recommended_options": [
                    "Qwen/Qwen2.5-Coder-7B-Instruct",
                    "Qwen/Qwen2.5-Coder-14B-Instruct",
                    state.code_fallback_model
                ]
            },
            "math_reasoning": {
                "primary": role_json(&state.math_model),
                "recommended_options": ["allenai/Olmo-3-7B-Instruct", state.math_large_model]
            },
            "embeddings": {
                "model": env::var("VERITAS_EMBEDDING_MODEL").unwrap_or_else(|_| "Muennighoff/SBERT-base-nli-v2".into()),
                "normalized": true,
                "cosine_search": "OpenSearch FAISS/HNSW"
            },
            "ontology_reasoning": {"graph": "Jena Fuseki SPARQL", "offline_reasoner": "Openllet"},
            "remote_fallback": {"enabled": state.remote_model_enabled, "base_url": state.remote_model_base_url, "model": state.remote_model_name}
        })),
    )
}

fn role_json(role: &ModelRole) -> Value {
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
    let runs = state.recent_runs.lock().await.clone();
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "runs_dir": state.runs_dir.display().to_string(),
            "recent_runs": runs,
            "message": "Run state is kept in memory for quick status and persisted in each run workspace as final_report.json."
        })),
    )
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
    match build_structured_plan(&state, &goal, size).await {
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

async fn build_structured_plan(state: &AppState, goal: &str, size: u32) -> Result<Value, ApiFailure> {
    let evidence = retrieve_evidence(state, goal, size, Some("hybrid")).await?;
    let hits = evidence.pointer("/hits/hits").and_then(Value::as_array).cloned().unwrap_or_default();
    if hits.is_empty() {
        return Err(ApiFailure::new(StatusCode::FAILED_DEPENDENCY, "plan.no_evidence", "No OpenSearch evidence hits were found for the prompt.", "Ingest arXiv papers or local PDFs first, then retry. Use `veritas search <query>` to verify retrieval."));
    }
    let formula_trace = run_formula_trace_query(state).await.unwrap_or_else(|error| {
        json!({"ok": false, "warning": {"code": error.code, "message": error.message, "remediation": error.remediation}})
    });
    let system = r#"You are the Veritas autonomous planner. You must return valid JSON only. No markdown. No prose outside JSON. Produce an evidence-backed plan using the provided retrieved evidence and ontology facts. Follow representation-first mathematical research: surface phenomenon, symbolic shadow, invariant, risk, plan, tasks, validation, build artifact. Do not claim production readiness until compile/test validation passes."#;
    let user = json!({
        "goal": goal,
        "required_json_schema": plan_schema_description(),
        "available_tools": ["retrieval", "sparql", "math_reasoning", "code_generation", "local_command", "test_runner"],
        "opensearch_evidence": compact_search_evidence(&evidence),
        "sparql_formula_trace": formula_trace,
        "hard_requirements": [
            "Return JSON only.",
            "Every step must have id, tool, description, input, and success_criteria.",
            "Include code_generation and test_runner steps.",
            "Include risks and validation gates."
        ]
    }).to_string();
    let plan = call_chat_model_json(state, &state.planner_model, system, &user).await?;
    validate_plan_schema(&plan)?;
    Ok(json!({
        "ok": true,
        "kind": "VeritasStructuredPlan",
        "status": "validated_structured_plan",
        "goal": goal,
        "model_route": {"planner": role_json(&state.planner_model), "code": role_json(&state.code_model), "math": role_json(&state.math_model)},
        "evidence": {"opensearch_faiss_hnsw": compact_search_evidence(&evidence), "jena_fuseki_formula_trace": formula_trace},
        "plan": plan
    }))
}

fn plan_schema_description() -> Value {
    json!({
        "objective": {"summary": "string", "desired_outcome": "string"},
        "steps": [{"id": "string", "tool": "retrieval|sparql|math_reasoning|code_generation|local_command|test_runner", "description": "string", "input": {}, "success_criteria": ["string"]}],
        "files_to_generate": [{"path": "relative/path", "purpose": "string"}],
        "commands_to_run": [{"command": "string", "purpose": "string"}],
        "risks": [{"risk": "string", "mitigation": "string"}],
        "validation_gates": [{"check": "string", "command": "optional string"}]
    })
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
    let run_id = format!("run-{}-{}", now_millis(), uuid::Uuid::new_v4().simple());
    let workspace = state.runs_dir.join(&run_id);
    fs::create_dir_all(&workspace).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.workspace_create", format!("Could not create run workspace: {error}"), "Ensure the API container has write access to /workspace/data/runs."))?;

    let language = req.language.unwrap_or_else(|| "rust".to_string());
    let max_retries = req.max_retries.unwrap_or(state.max_retries).min(5);
    let plan_envelope = build_structured_plan(state, &req.goal, req.size.unwrap_or(8)).await?;
    let plan = plan_envelope.get("plan").cloned().unwrap_or_else(|| json!({}));
    let mut tool_calls = Vec::new();
    let mut files_changed: Vec<String> = Vec::new();
    let mut commands_run: Vec<Value> = Vec::new();
    let mut validation_results: Vec<Value> = Vec::new();
    let mut retry_history: Vec<Value> = Vec::new();

    let tool_outputs = execute_planner_selected_tools(state, &plan, &req.goal, &mut tool_calls).await;
    let mut last_error_summary = String::new();
    let mut code_package = json!({});
    let mut final_status = "failed".to_string();
    let mut attempts_performed = 0usize;

    for attempt in 0..=max_retries {
        attempts_performed = attempt + 1;
        let code_prompt = build_code_generation_prompt(&req.goal, &language, &plan, &plan_envelope["evidence"], &tool_outputs, &last_error_summary, attempt);
        let generated = call_chat_model_json(state, &state.code_model, codegen_system_prompt(), &code_prompt).await?;
        validate_codegen_schema(&generated)?;
        write_generated_files(&workspace, &generated, &mut files_changed).await?;
        code_package = generated.clone();

        let commands = commands_for_run(&workspace, &language, &generated);
        let mut all_passed = true;
        let mut attempt_results = Vec::new();
        for command in commands {
            let result = run_command(&workspace, &command, state.command_timeout_secs).await;
            commands_run.push(result.clone());
            attempt_results.push(result.clone());
            if !result.get("success").and_then(Value::as_bool).unwrap_or(false) {
                all_passed = false;
            }
        }
        validation_results.push(json!({"attempt": attempt, "results": attempt_results}));
        if all_passed {
            final_status = "production_candidate_validated".to_string();
            break;
        }
        last_error_summary = validation_results.last().cloned().unwrap_or_else(|| json!({})).to_string();
        retry_history.push(json!({
            "attempt": attempt,
            "reason": "compile_or_test_failure",
            "feedback_sent_to_code_model": last_error_summary
        }));
    }

    let report = json!({
        "ok": final_status == "production_candidate_validated",
        "kind": "VeritasAutonomousRunReport",
        "run_id": run_id,
        "workspace": workspace.display().to_string(),
        "original_task": req.goal,
        "language": language,
        "generated_plan": plan,
        "model_routes_used": {"planner": role_json(&state.planner_model), "code": role_json(&state.code_model), "math": role_json(&state.math_model)},
        "tool_calls_performed": tool_calls,
        "files_changed": files_changed,
        "commands_run": commands_run,
        "validation_results": validation_results,
        "retries_performed": retry_history.len(),
        "retry_history": retry_history,
        "generated_package_status": final_status,
        "final_status": final_status,
        "remaining_limitations": if final_status == "production_candidate_validated" { json!([]) } else { json!(["Generated code did not pass compile/test validation within the configured retry limit."]) },
        "code_model_output": code_package,
    });
    let report_text = serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string());
    fs::write(workspace.join("final_report.json"), report_text).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "run.report_write", format!("Could not write final report: {error}"), "Check write permissions for the run workspace."))?;
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
                "math_reasoning" => {
                    let input = step.get("input").cloned().unwrap_or_else(|| json!({"goal": goal})).to_string();
                    let result = call_chat_model_text(state, &state.math_model, "You are Veritas Math Reasoner. Return concise mathematical reasoning summaries only, with assumptions, invariants, and failure cases. Do not expose private chain-of-thought.", &input).await;
                    tool_calls.push(json!({"tool": "math_reasoning", "success": result.is_ok()}));
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

fn build_code_generation_prompt(goal: &str, language: &str, plan: &Value, evidence: &Value, tool_outputs: &Value, last_error_summary: &str, attempt: usize) -> String {
    json!({
        "goal": goal,
        "language": language,
        "attempt": attempt,
        "plan": plan,
        "evidence": evidence,
        "tool_outputs": tool_outputs,
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
            "files": [{"path": "relative/path", "content": "complete file content"}],
            "commands": [{"command": "shell command to compile/test in workspace", "purpose": "why this validates the output"}],
            "assumptions": ["string"],
            "validation_summary": "string"
        },
        "hard_requirements": [
            "Return JSON only. No markdown outside JSON.",
            "Generate complete files, not snippets.",
            "For Rust, include Cargo.toml, src/lib.rs, and at least one test.",
            "Commands must compile and run tests.",
            "Do not claim GPU support unless actual GPU code is generated and tested. Prefer CPU-safe implementation with explicit extension points."
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
                if item.get("content").and_then(Value::as_str).map(|s| s.trim().is_empty()).unwrap_or(true) { errors.push("each file needs non-empty content"); }
            }
        }
        _ => errors.push("files must be a non-empty array"),
    }
    if errors.is_empty() { Ok(()) } else { Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.schema_invalid", "Code model returned JSON that failed the Veritas codegen schema.", "Use a stronger code model or reduce temperature; inspect model output in the error details.").with_details(json!({"errors": errors, "output": output}))) }
}

async fn write_generated_files(workspace: &Path, generated: &Value, files_changed: &mut Vec<String>) -> Result<(), ApiFailure> {
    let files = generated.get("files").and_then(Value::as_array).ok_or_else(|| ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.files_missing", "Code model output contained no files.", "Retry with a model that follows the Veritas JSON schema."))?;
    for file in files {
        let rel = file.get("path").and_then(Value::as_str).unwrap_or_default();
        let content = file.get("content").and_then(Value::as_str).unwrap_or_default();
        let target = safe_join(workspace, rel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.mkdir", format!("Could not create directory for {rel}: {error}"), "Check run workspace permissions."))?;
        }
        fs::write(&target, content).await.map_err(|error| ApiFailure::new(StatusCode::INTERNAL_SERVER_ERROR, "codegen.write_file", format!("Could not write generated file {rel}: {error}"), "Check run workspace permissions."))?;
        files_changed.push(rel.to_string());
    }
    Ok(())
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, ApiFailure> {
    let path = Path::new(rel);
    if path.is_absolute() || rel.contains("..") {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "codegen.unsafe_path", format!("Generated file path is unsafe: {rel}"), "Regenerate code. Generated paths must be relative and must not contain `..`."));
    }
    Ok(root.join(path))
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
    let started = now_millis();
    let result = timeout(
        Duration::from_secs(timeout_secs),
        Command::new("sh").arg("-lc").arg(command).current_dir(workspace).output(),
    ).await;
    match result {
        Ok(Ok(output)) => json!({
            "command": command,
            "success": output.status.success(),
            "exit_code": output.status.code(),
            "duration_ms": now_millis().saturating_sub(started),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string()
        }),
        Ok(Err(error)) => json!({
            "command": command,
            "success": false,
            "duration_ms": now_millis().saturating_sub(started),
            "error": format!("Failed to launch command: {error}"),
            "remediation": "Ensure required toolchains are installed in the API container. Rust packages require cargo in Dockerfile.api."
        }),
        Err(_) => json!({
            "command": command,
            "success": false,
            "duration_ms": now_millis().saturating_sub(started),
            "error": format!("Command timed out after {timeout_secs}s"),
            "remediation": "Increase VERITAS_COMMAND_TIMEOUT_SECS or simplify generated tests."
        }),
    }
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
    let url = format!("{}/{}/_search", state.opensearch_url.trim_end_matches('/'), state.opensearch_index);
    let response = state.http.post(&url).json(&body).send().await.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "search.transport", format!("OpenSearch request failed before the service returned a response: {error}"), "Check OpenSearch readiness with `veritas ready` and inspect `docker compose logs opensearch`."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "search.upstream", format!("OpenSearch returned HTTP {}", status.as_u16()), "Check OpenSearch logs, index mapping, and whether data has been ingested.").with_details(body));
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
    let raw = call_chat_model_raw(state, role, system, user, false).await?;
    let content = extract_model_content(&raw).unwrap_or_else(|| raw.to_string());
    Ok(json!({"raw": raw, "content": content}))
}

async fn call_chat_model_json(state: &AppState, role: &ModelRole, system: &str, user: &str) -> Result<Value, ApiFailure> {
    let raw = call_chat_model_raw(state, role, system, user, true).await?;
    let content = extract_model_content(&raw).unwrap_or_else(|| raw.to_string());
    match parse_json_object_from_text(&content) {
        Ok(value) => Ok(value),
        Err(first_error) => {
            let repair_user = json!({"invalid_output": content, "parse_error": first_error, "instruction": "Return the same content as a single valid JSON object. No markdown. No prose."}).to_string();
            let repaired_raw = call_chat_model_raw(state, role, "You repair invalid JSON. Return JSON only.", &repair_user, true).await?;
            let repaired = extract_model_content(&repaired_raw).unwrap_or_else(|| repaired_raw.to_string());
            parse_json_object_from_text(&repaired).map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "model.invalid_json", format!("Model did not return valid JSON after repair: {error}"), "Use a stronger model, reduce temperature, or inspect vLLM output.").with_details(json!({"first_error": first_error, "raw": raw, "repair_raw": repaired_raw})))
        }
    }
}

async fn call_chat_model_raw(state: &AppState, role: &ModelRole, system: &str, user: &str, json_mode: bool) -> Result<Value, ApiFailure> {
    let url = format!("{}/v1/chat/completions", role.url.trim_end_matches('/'));
    let mut payload = json!({
        "model": role.served_model_name,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "temperature": role.temperature,
        "top_p": role.top_p,
        "max_tokens": role.max_tokens
    });
    if json_mode {
        payload["response_format"] = json!({"type": "json_object"});
    }
    let fut = state.http.post(&url).json(&payload).send();
    let response = timeout(Duration::from_secs(role.timeout_secs), fut).await.map_err(|_| ApiFailure::new(StatusCode::GATEWAY_TIMEOUT, "vllm.timeout", format!("vLLM request for role {} timed out after {}s", role.role, role.timeout_secs), "Increase the role timeout or use a smaller/faster model."))?.map_err(|error| ApiFailure::new(StatusCode::BAD_GATEWAY, "vllm.transport", format!("vLLM request failed before response: {error}"), "Start the selected vLLM Docker Compose profile or set the role URL to a reachable OpenAI-compatible endpoint."))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(ApiFailure::new(StatusCode::BAD_GATEWAY, "vllm.upstream", format!("vLLM returned HTTP {}", status.as_u16()), "Check model ID, Hugging Face token, GPU memory, and vLLM logs.").with_details(body));
    }
    Ok(body)
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

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}
