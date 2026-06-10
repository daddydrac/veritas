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
use std::{env, net::SocketAddr, sync::Arc};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
struct ModelRole {
    role: &'static str,
    url: String,
    model: String,
    served_model_name: String,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Clone)]
struct AppState {
    http: Client,
    opensearch_url: String,
    opensearch_index: String,
    opensearch_vector_field: String,
    fuseki_query_url: String,
    fuseki_ping_url: String,
    qdrant_url: String,
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
    model: String,
    dimension: usize,
    normalized: bool,
    vectors: Vec<Vec<f32>>,
    norms: Vec<f32>,
}

#[derive(Debug, Serialize)]
struct Health {
    service: &'static str,
    status: &'static str,
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
        qdrant_url: env::var("VERITAS_QDRANT_URL")
            .unwrap_or_else(|_| "http://qdrant:6333".into()),
        embedding_url: env::var("VERITAS_EMBEDDING_URL")
            .unwrap_or_else(|_| "http://embedding:8090".into()),
        require_models: bool_env("VERITAS_REQUIRE_MODELS", false),
        planner_model: ModelRole {
            role: "planner",
            url: env::var("VERITAS_PLANNER_VLLM_URL")
                .unwrap_or_else(|_| "http://vllm-planner:8000".into()),
            model: env::var("VERITAS_PLANNER_MODEL")
                .unwrap_or_else(|_| "Qwen/Qwen2.5-Coder-7B-Instruct".into()),
            served_model_name: env::var("VERITAS_PLANNER_SERVED_MODEL_NAME")
                .unwrap_or_else(|_| "veritas-planner".into()),
            temperature: float_env("VERITAS_PLANNER_TEMPERATURE", 0.1),
            max_tokens: uint_env("VERITAS_PLANNER_MAX_TOKENS", 1800),
        },
        code_model: ModelRole {
            role: "code_generation",
            url: env::var("VERITAS_CODE_VLLM_URL")
                .unwrap_or_else(|_| "http://vllm-code:8000".into()),
            model: env::var("VERITAS_CODE_MODEL")
                .unwrap_or_else(|_| "Qwen/Qwen2.5-Coder-14B-Instruct".into()),
            served_model_name: env::var("VERITAS_CODE_SERVED_MODEL_NAME")
                .unwrap_or_else(|_| "veritas-code".into()),
            temperature: float_env("VERITAS_CODE_TEMPERATURE", 0.05),
            max_tokens: uint_env("VERITAS_CODE_MAX_TOKENS", 4096),
        },
        math_model: ModelRole {
            role: "math_reasoning",
            url: env::var("VERITAS_MATH_VLLM_URL")
                .unwrap_or_else(|_| "http://vllm-math:8000".into()),
            model: env::var("VERITAS_MATH_MODEL")
                .unwrap_or_else(|_| "allenai/Olmo-3-7B-Instruct".into()),
            served_model_name: env::var("VERITAS_MATH_SERVED_MODEL_NAME")
                .unwrap_or_else(|_| "veritas-math".into()),
            temperature: float_env("VERITAS_MATH_TEMPERATURE", 0.1),
            max_tokens: uint_env("VERITAS_MATH_MAX_TOKENS", 4096),
        },
        code_fallback_model: env::var("VERITAS_CODE_FALLBACK_MODEL")
            .unwrap_or_else(|_| "deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct".into()),
        math_large_model: env::var("VERITAS_MATH_LARGE_MODEL")
            .unwrap_or_else(|_| "allenai/Olmo-3.1-32B-Instruct".into()),
        remote_model_enabled: bool_env("VERITAS_REMOTE_MODEL_ENABLED", false),
        remote_model_base_url: env::var("VERITAS_REMOTE_MODEL_BASE_URL").unwrap_or_default(),
        remote_model_name: env::var("VERITAS_REMOTE_MODEL_NAME").unwrap_or_default(),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/models", get(models))
        .route("/graph/status", get(graph_status))
        .route("/sparql", post(sparql))
        .route("/search", post(search))
        .route("/plan", post(plan))
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
    let qdrant = probe(&state.http, &state.qdrant_url, "qdrant").await;
    let embedding = probe(
        &state.http,
        &format!("{}/health", state.embedding_url.trim_end_matches('/')),
        "embedding",
    )
    .await;
    let planner = probe_model(&state.http, &state.planner_model).await;
    let code = probe_model(&state.http, &state.code_model).await;
    let math = probe_model(&state.http, &state.math_model).await;
    let base_ok = opensearch["ok"].as_bool().unwrap_or(false)
        && fuseki["ok"].as_bool().unwrap_or(false)
        && qdrant["ok"].as_bool().unwrap_or(false)
        && embedding["ok"].as_bool().unwrap_or(false);
    let model_ok = planner["ok"].as_bool().unwrap_or(false)
        && code["ok"].as_bool().unwrap_or(false)
        && math["ok"].as_bool().unwrap_or(false);
    let ok = base_ok && (!state.require_models || model_ok);
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "service": "veritas-api",
            "ready": ok,
            "model_services_required": state.require_models,
            "checks": {
                "opensearch": opensearch,
                "fuseki": fuseki,
                "qdrant": qdrant,
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
            "rust_integration": "Rust API and CLI call vLLM HTTP endpoints; vLLM downloads Hugging Face models into the hf-cache Docker volume.",
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
                "recommended_options": [
                    "allenai/Olmo-3-7B-Instruct",
                    state.math_large_model
                ]
            },
            "embeddings": {
                "model": env::var("VERITAS_EMBEDDING_MODEL").unwrap_or_else(|_| "Muennighoff/SBERT-base-nli-v2".into()),
                "normalized": true,
                "cosine_search": "OpenSearch FAISS/HNSW"
            },
            "ontology_reasoning": {
                "graph": "Jena Fuseki SPARQL",
                "offline_reasoner": "Openllet"
            },
            "remote_fallback": {
                "enabled": state.remote_model_enabled,
                "base_url": state.remote_model_base_url,
                "model": state.remote_model_name
            }
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
        "max_tokens": role.max_tokens
    })
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
            "ontology": {
                "name": "Veritas / Invariant Forge OWL-DL",
                "namespace": "https://github.com/daddydrac/veritas/ontology#"
            },
            "reasoner": {"name": "Openllet"},
            "graph": {"name": "Fuseki", "query_url": state.fuseki_query_url},
            "vector_memory": {"name": "OpenSearch FAISS/HNSW", "index": state.opensearch_index},
            "warnings": warnings
        })),
    )
}

async fn sparql_count(state: &AppState, class_name: &str, warnings: &mut Vec<Value>) -> Option<u64> {
    let query = format!(
        "PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>\nSELECT (COUNT(?s) AS ?count) WHERE {{ ?s a veritas:{class_name} . }}"
    );
    let response = match state
        .http
        .post(&state.fuseki_query_url)
        .header("accept", "application/sparql-results+json")
        .form(&[("query", query)])
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            warnings.push(json!({
                "stage": "graph_status.transport",
                "class": class_name,
                "message": format!("Fuseki count query failed before response: {error}"),
                "remediation": "Start Fuseki and upload the ontology with `veritas upload-ontology`."
            }));
            return None;
        }
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        warnings.push(json!({
            "stage": "graph_status.upstream",
            "class": class_name,
            "message": format!("Fuseki count query returned HTTP {status}: {}", text.chars().take(300).collect::<String>()),
            "remediation": "Verify the Veritas dataset and ontology graph are loaded."
        }));
        return None;
    }
    let payload: Value = match response.json().await {
        Ok(value) => value,
        Err(error) => {
            warnings.push(json!({
                "stage": "graph_status.parse",
                "class": class_name,
                "message": format!("Fuseki count query response could not be decoded: {error}"),
                "remediation": "Check Fuseki SPARQL result format."
            }));
            return None;
        }
    };
    payload
        .pointer("/results/bindings/0/count/value")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
}

async fn probe(http: &Client, url: &str, service: &str) -> Value {
    match http.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            json!({
                "service": service,
                "ok": status.is_success(),
                "url": url,
                "http_status": status.as_u16(),
                "message": if status.is_success() { "reachable" } else { "service responded with non-success HTTP status" }
            })
        }
        Err(error) => json!({
            "service": service,
            "ok": false,
            "url": url,
            "error": error.to_string(),
            "message": "service is unreachable from API container"
        }),
    }
}

async fn probe_model(http: &Client, role: &ModelRole) -> Value {
    let url = format!("{}/v1/models", role.url.trim_end_matches('/'));
    match http.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            json!({
                "service": role.role,
                "ok": status.is_success(),
                "url": url,
                "model": role.model,
                "served_model_name": role.served_model_name,
                "http_status": status.as_u16(),
                "message": if status.is_success() { "vLLM OpenAI-compatible model endpoint reachable" } else { "vLLM responded with non-success HTTP status" }
            })
        }
        Err(error) => json!({
            "service": role.role,
            "ok": false,
            "url": url,
            "model": role.model,
            "served_model_name": role.served_model_name,
            "error": error.to_string(),
            "message": "vLLM endpoint is unavailable; start model profiles or configure a remote fallback"
        }),
    }
}

async fn sparql(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SparqlRequest>,
) -> impl IntoResponse {
    if req.query.trim().is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "sparql.validation",
            "SPARQL query is empty.",
            "Provide a non-empty SPARQL query string in JSON field `query`.",
        );
    }
    let res = state
        .http
        .post(&state.fuseki_query_url)
        .header("accept", "application/sparql-results+json")
        .form(&[("query", req.query)])
        .send()
        .await;
    match res {
        Ok(r) => upstream_response("sparql.query", "fuseki", &state.fuseki_query_url, r).await,
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            "sparql.transport",
            &format!("Fuseki request failed before the service returned a response: {e}"),
            "Check Fuseki readiness with `veritas ready` and inspect `docker compose logs fuseki`.",
        ),
    }
}

async fn search(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    if req.query.trim().is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "search.validation",
            "Search query is empty.",
            "Provide a non-empty query string.",
        );
    }
    let index = req.index.unwrap_or_else(|| state.opensearch_index.clone());
    let size = req.size.unwrap_or(10);
    let mode = req.mode.unwrap_or_else(|| "hybrid".into());
    let body_result = if mode == "lexical" {
        Ok(lexical_query(&req.query, size))
    } else {
        match embed_query(&state, &req.query).await {
            Ok(vector) if mode == "semantic" => Ok(vector_query(&state.opensearch_vector_field, vector, size)),
            Ok(vector) => Ok(hybrid_query(&req.query, &state.opensearch_vector_field, vector, size)),
            Err(response) => Err(response),
        }
    };
    let body = match body_result {
        Ok(body) => body,
        Err(response) => return response,
    };
    let url = format!("/{}/_search", index);
    let full_url = format!("{}{}", state.opensearch_url.trim_end_matches('/'), url);
    let res = state.http.post(&full_url).json(&body).send().await;
    match res {
        Ok(r) => upstream_response("search.query", "opensearch", &full_url, r).await,
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            "search.transport",
            &format!("OpenSearch request failed before the service returned a response: {e}"),
            "Check OpenSearch readiness with `veritas ready` and inspect `docker compose logs opensearch`.",
        ),
    }
}

fn lexical_query(query: &str, size: u32) -> Value {
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    {"multi_match": {"query": query, "fields": ["text^3", "title^4", "metadata.summary^2"]}},
                    {"nested": {"path": "formulas", "query": {"match": {"formulas.latex": query}}, "score_mode": "max"}}
                ],
                "minimum_should_match": 1
            }
        }
    })
}

fn hybrid_query(query: &str, vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(vector_field.to_string(), json!({"vector": vector, "k": size}));
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    {"knn": Value::Object(knn_body)},
                    {"multi_match": {"query": query, "fields": ["text^3", "title^4", "metadata.summary^2"]}},
                    {"nested": {"path": "formulas", "query": {"match": {"formulas.latex": query}}, "score_mode": "max"}}
                ],
                "minimum_should_match": 1
            }
        }
    })
}

fn vector_query(vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(vector_field.to_string(), json!({"vector": vector, "k": size}));
    json!({"size": size, "query": {"knn": Value::Object(knn_body)}})
}

async fn embed_query(state: &AppState, query: &str) -> Result<Vec<f32>, (StatusCode, Json<Value>)> {
    let url = format!("{}/embed", state.embedding_url.trim_end_matches('/'));
    let res = state
        .http
        .post(&url)
        .json(&json!({"texts": [query], "normalize": true, "batch_size": 1}))
        .send()
        .await
        .map_err(|error| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "embedding.transport",
                &format!("Embedding request failed before the service returned a response: {error}"),
                "Check `docker compose logs embedding` and ensure the embedding service is healthy.",
            )
        })?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(error_response(
            StatusCode::BAD_GATEWAY,
            "embedding.upstream",
            &format!("Embedding service returned HTTP {}: {}", status.as_u16(), text),
            "Inspect the embedding service logs and retry with a non-empty query.",
        ));
    }
    let payload: EmbedResponse = res.json().await.map_err(|error| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "embedding.parse",
            &format!("Embedding response could not be decoded: {error}"),
            "Check embedding service compatibility and response schema.",
        )
    })?;
    let vector = payload.vectors.into_iter().next().ok_or_else(|| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "embedding.empty_vector",
            "Embedding service returned no query vector.",
            "Retry and inspect `docker compose logs embedding`.",
        )
    })?;
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if (norm - 1.0).abs() > 0.001 {
        return Err(error_response(
            StatusCode::BAD_GATEWAY,
            "embedding.norm",
            &format!("Query embedding is not normalized for cosine search: norm={norm:.6}"),
            "Ensure VERITAS_EMBEDDING_NORMALIZE=true and the embedding service uses normalize_embeddings=True.",
        ));
    }
    Ok(vector)
}

async fn llm_chat(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> impl IntoResponse {
    let role = input.get("role").and_then(Value::as_str).unwrap_or("planner");
    let prompt = match input.get("prompt").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => value,
        None => return error_response(StatusCode::BAD_REQUEST, "llm.validation", "Missing non-empty `prompt`.", "Pass a prompt and optional role: planner, code, or math."),
    };
    let model = match role {
        "code" | "code_generation" => &state.code_model,
        "math" | "math_reasoning" => &state.math_model,
        _ => &state.planner_model,
    };
    match call_chat_model(
        &state,
        model,
        "You are Veritas. Return precise, evidence-backed, implementation-oriented output.",
        prompt,
        None,
    )
    .await
    {
        Ok(value) => (StatusCode::OK, Json(json!({"ok": true, "role": role, "model": role_json(model), "result": value}))),
        Err(value) => (StatusCode::BAD_GATEWAY, Json(value)),
    }
}

async fn plan(
    State(state): State<Arc<AppState>>,
    Json(input): Json<Value>,
) -> impl IntoResponse {
    let goal = match input
        .get("goal")
        .or_else(|| input.get("prompt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "plan.validation",
                "Plan request is missing a non-empty `goal` or `prompt`.",
                "Ask Veritas what you want built, analyzed, or converted from research into code.",
            )
        }
    };
    let size = input.get("size").and_then(Value::as_u64).unwrap_or(8).min(50) as u32;
    let vector = match embed_query(&state, &goal).await {
        Ok(vector) => vector,
        Err(response) => return response,
    };
    let search_body = hybrid_query(&goal, &state.opensearch_vector_field, vector, size);
    let search_url = format!("{}/{}/_search", state.opensearch_url.trim_end_matches('/'), state.opensearch_index);
    let search_response = match state.http.post(&search_url).json(&search_body).send().await {
        Ok(response) => response,
        Err(error) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "plan.evidence_transport",
                &format!("OpenSearch evidence retrieval failed before response: {error}"),
                "Run `veritas ready`, ingest papers/PDFs, and inspect `docker compose logs opensearch embedding`.",
            )
        }
    };
    if !search_response.status().is_success() {
        return upstream_response("plan.evidence", "opensearch", &search_url, search_response).await;
    }
    let search_payload: Value = match search_response.json().await {
        Ok(value) => value,
        Err(error) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "plan.evidence_parse",
                &format!("OpenSearch evidence response could not be decoded: {error}"),
                "Inspect OpenSearch logs and verify response shape.",
            )
        }
    };
    let total_hits = search_payload
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    if total_hits == 0 {
        return error_response(
            StatusCode::FAILED_DEPENDENCY,
            "plan.no_evidence",
            "No OpenSearch evidence hits were found for the prompt.",
            "Ingest arXiv papers or local PDFs first, then retry. Use `veritas search <query>` to verify retrieval.",
        );
    }

    let formula_query = r#"
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
    let graph_result = state
        .http
        .post(&state.fuseki_query_url)
        .header("accept", "application/sparql-results+json")
        .form(&[("query", formula_query)])
        .send()
        .await;
    let mut warnings: Vec<Value> = Vec::new();
    let graph_payload = match graph_result {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(value) => value,
            Err(error) => {
                warnings.push(json!({"stage": "plan.graph_parse", "message": format!("Fuseki graph response could not be decoded: {error}"), "remediation": "Inspect Fuseki logs and verify SPARQL endpoint returns JSON."}));
                json!({})
            }
        },
        Ok(response) => {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            warnings.push(json!({"stage": "plan.graph_query", "message": format!("Fuseki returned HTTP {status}: {}", text.chars().take(500).collect::<String>()), "remediation": "Upload ontology and ingest PDFs so formula SymbolicShadow triples exist."}));
            json!({})
        }
        Err(error) => {
            warnings.push(json!({"stage": "plan.graph_transport", "message": format!("Fuseki SPARQL request failed before response: {error}"), "remediation": "Run `veritas ready`; inspect `docker compose logs fuseki`."}));
            json!({})
        }
    };

    let deterministic_analysis = json!({
        "surface_phenomenon": "Research prompt grounded against indexed PDF chunks and formula symbolic shadows.",
        "candidate_representation_map": "PDF text + LaTeX formulas -> evidence chunks -> ontology graph -> implementation plan.",
        "candidate_invariants": [
            "formula expressions remain linked to source chunks",
            "generated code must cite evidence chunks",
            "tests must cover extracted assumptions, preconditions, postconditions, and numerical tolerance"
        ],
        "risk_register": [
            "MathematicalRisk: formulas may be underspecified or parsed incorrectly",
            "TechnicalRisk: generated code may not preserve numerical invariants",
            "OperationalRisk: generated package may not match CPU/GPU runtime"
        ]
    });

    let planner_prompt = json!({
        "goal": goal,
        "instructions": "Return an evidence-backed implementation plan. Follow representation-first mathematical reasoning. Include assumptions, invariants, risks, validation gates, and code generation strategy.",
        "opensearch_evidence": search_payload,
        "sparql_formula_trace": graph_payload,
        "deterministic_analysis": deterministic_analysis,
    }).to_string();
    let llm_plan = match call_chat_model(
        &state,
        &state.planner_model,
        "You are the Veritas planner. You must ground every claim in retrieved evidence or ontology facts. Do not claim production readiness until validation gates pass.",
        &planner_prompt,
        Some(&mut warnings),
    ).await {
        Ok(value) => json!({"ok": true, "provider": "vllm", "model": role_json(&state.planner_model), "result": value}),
        Err(value) => {
            warnings.push(json!({
                "stage": "plan.llm_planner",
                "message": "Planner vLLM call failed; deterministic scaffold plan returned instead.",
                "details": value,
                "remediation": "Start vLLM planner with `docker compose --profile models up -d vllm-planner` or configure remote fallback."
            }));
            json!({"ok": false, "provider": "vllm", "model": role_json(&state.planner_model), "fallback": "deterministic_analysis"})
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "kind": "VeritasEvidenceBackedPlan",
            "status": "evidence_backed_planning_draft",
            "goal": goal,
            "models": {
                "planner": role_json(&state.planner_model),
                "code": role_json(&state.code_model),
                "math": role_json(&state.math_model),
                "embedding": env::var("VERITAS_EMBEDDING_MODEL").unwrap_or_else(|_| "Muennighoff/SBERT-base-nli-v2".into())
            },
            "evidence": {"opensearch_faiss_hnsw": search_payload, "jena_fuseki_formula_trace": graph_payload, "warnings": warnings},
            "llm_planner": llm_plan,
            "representation_first_analysis": deterministic_analysis,
            "execution_plan": [
                "review retrieved evidence and formula trace",
                "extract assumptions, domains, invariants, and constraints",
                "generate package with tests and validation report using `veritas generate-code`",
                "run control-flow back-check and package tests",
                "mark result production-ready only after validation gates pass"
            ],
            "next_actions": [
                "veritas generate-code --language rust --prompt '<same prompt>'",
                "veritas search '<related query>'",
                "veritas sparql '<cross-domain query>'"
            ],
            "minimum_acceptance_gates": {
                "evidence_required": true,
                "acceptance_criteria_required": true,
                "risk_register_required": true,
                "control_flow_backcheck_required": true,
                "tests_required": true,
                "distribution_artifact_required": true,
                "normalized_embeddings_required": true
            }
        })),
    )
}

async fn call_chat_model(
    state: &AppState,
    role: &ModelRole,
    system: &str,
    user: &str,
    _warnings: Option<&mut Vec<Value>>,
) -> Result<Value, Value> {
    let url = format!("{}/v1/chat/completions", role.url.trim_end_matches('/'));
    let payload = json!({
        "model": role.served_model_name,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": role.temperature,
        "max_tokens": role.max_tokens
    });
    let response = state.http.post(&url).json(&payload).send().await.map_err(|error| {
        json!({
            "ok": false,
            "error": {"code": "vllm.transport", "message": format!("vLLM request failed before response: {error}"), "stage": "model.chat", "component": role.role, "remediation": "Start the selected vLLM Docker Compose profile or set the role URL to a reachable OpenAI-compatible endpoint."}
        })
    })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        return Err(json!({
            "ok": false,
            "error": {"code": "vllm.upstream", "http_status": status.as_u16(), "stage": "model.chat", "component": role.role, "message": "vLLM returned an error.", "details": body, "remediation": "Check model ID, Hugging Face token, GPU memory, and vLLM logs."}
        }));
    }
    Ok(body)
}

async fn upstream_response(
    operation: &str,
    service: &str,
    url: &str,
    response: reqwest::Response,
) -> (StatusCode, Json<Value>) {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    let http_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if status.is_success() {
        (http_status, Json(json!({"ok": true, "operation": operation, "service": service, "upstream_url": url, "result": body})))
    } else {
        (http_status, Json(json!({
            "ok": false,
            "operation": operation,
            "service": service,
            "upstream_url": url,
            "http_status": status.as_u16(),
            "message": "Upstream service returned an error.",
            "details": body,
            "remediation": "Check service logs, endpoint configuration, and whether data has been ingested."
        })))
    }
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    remediation: &str,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"ok": false, "error": {"code": code, "message": message, "remediation": remediation}})),
    )
}
