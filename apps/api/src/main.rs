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
struct AppState {
    http: Client,
    opensearch_url: String,
    opensearch_index: String,
    opensearch_vector_field: String,
    fuseki_query_url: String,
    fuseki_ping_url: String,
    qdrant_url: String,
    embedding_url: String,
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
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/sparql", post(sparql))
        .route("/search", post(search))
        .route("/plan", post(plan))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Veritas API listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
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
    let ok = opensearch["ok"].as_bool().unwrap_or(false)
        && fuseki["ok"].as_bool().unwrap_or(false)
        && qdrant["ok"].as_bool().unwrap_or(false)
        && embedding["ok"].as_bool().unwrap_or(false);
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
            "checks": {
                "opensearch": opensearch,
                "fuseki": fuseki,
                "qdrant": qdrant,
                "embedding": embedding
            },
            "help": if ok { "All required services are reachable." } else { "Run `docker compose ps` and `docker compose logs --tail=200` to locate the failing service." }
        })),
    )
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
            &format!("SPARQL request failed before Fuseki returned a response: {e}"),
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
            "Provide a non-empty search query string in JSON field `query`.",
        );
    }

    let index = req.index.unwrap_or_else(|| state.opensearch_index.clone());
    let size = req.size.unwrap_or(10);
    let mode = req.mode.unwrap_or_else(|| "semantic".into());
    let body_result = if mode == "lexical" {
        Ok(lexical_query(&req.query, size))
    } else {
        match embed_query(&state, &req.query).await {
            Ok(vector) => Ok(vector_query(&state.opensearch_vector_field, vector, size)),
            Err(response) => Err(response),
        }
    };

    let body = match body_result {
        Ok(body) => body,
        Err(response) => return response,
    };
    let url = format!("{}/{}/_search", state.opensearch_url.trim_end_matches('/'), index);
    let res = state.http.post(&url).json(&body).send().await;
    match res {
        Ok(r) => upstream_response("search.query", "opensearch", &url, r).await,
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
                    {
                        "multi_match": {
                            "query": query,
                            "fields": ["text^3", "title^4", "metadata.summary^2"]
                        }
                    },
                    {
                        "nested": {
                            "path": "formulas",
                            "query": {"match": {"formulas.latex": query}},
                            "score_mode": "max"
                        }
                    }
                ],
                "minimum_should_match": 1
            }
        }
    })
}

fn hybrid_query(query: &str, vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(
        vector_field.to_string(),
        json!({
            "vector": vector,
            "k": size
        }),
    );
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    {"knn": Value::Object(knn_body)},
                    {
                        "multi_match": {
                            "query": query,
                            "fields": ["text^3", "title^4", "metadata.summary^2"]
                        }
                    },
                    {
                        "nested": {
                            "path": "formulas",
                            "query": {"match": {"formulas.latex": query}},
                            "score_mode": "max"
                        }
                    }
                ],
                "minimum_should_match": 1
            }
        }
    })
}

fn vector_query(vector_field: &str, vector: Vec<f32>, size: u32) -> Value {
    let mut knn_body = serde_json::Map::new();
    knn_body.insert(
        vector_field.to_string(),
        json!({
            "vector": vector,
            "k": size
        }),
    );
    json!({
        "size": size,
        "query": {
            "knn": Value::Object(knn_body)
        }
    })
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

    let size = input
        .get("size")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .min(50) as u32;

    let vector = match embed_query(&state, &goal).await {
        Ok(vector) => vector,
        Err(response) => return response,
    };
    let search_body = hybrid_query(&goal, &state.opensearch_vector_field, vector, size);
    let search_url = format!(
        "{}/{}/_search",
        state.opensearch_url.trim_end_matches('/'),
        state.opensearch_index
    );
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
                warnings.push(json!({
                    "stage": "plan.graph_parse",
                    "message": format!("Fuseki graph response could not be decoded: {error}"),
                    "remediation": "Inspect Fuseki logs and verify SPARQL endpoint returns application/sparql-results+json."
                }));
                json!({})
            }
        },
        Ok(response) => {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            warnings.push(json!({
                "stage": "plan.graph_query",
                "message": format!("Fuseki returned HTTP {status}: {}", &text.chars().take(500).collect::<String>()),
                "remediation": "Upload ontology and ingest PDFs so formula SymbolicShadow triples exist."
            }));
            json!({})
        }
        Err(error) => {
            warnings.push(json!({
                "stage": "plan.graph_transport",
                "message": format!("Fuseki SPARQL request failed before response: {error}"),
                "remediation": "Run `veritas ready`; inspect `docker compose logs fuseki`."
            }));
            json!({})
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "kind": "VeritasEvidenceBackedPlan",
            "status": "evidence_backed_planning_draft",
            "goal": goal,
            "evidence": {
                "opensearch_faiss_hnsw": search_payload,
                "jena_fuseki_formula_trace": graph_payload,
                "warnings": warnings
            },
            "representation_first_analysis": {
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
            },
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
        (
            http_status,
            Json(json!({
                "ok": true,
                "operation": operation,
                "service": service,
                "upstream_url": url,
                "result": body
            })),
        )
    } else {
        (
            http_status,
            Json(json!({
                "ok": false,
                "operation": operation,
                "service": service,
                "upstream_url": url,
                "http_status": status.as_u16(),
                "message": "Upstream service returned an error.",
                "details": body,
                "remediation": "Check service logs, endpoint configuration, and whether data has been ingested."
            })),
        )
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
        Json(json!({
            "ok": false,
            "error": {
                "code": code,
                "message": message,
                "remediation": remediation
            }
        })),
    )
}
