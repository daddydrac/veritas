use crate::schemas::{schema_json, SchemaKey};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::Mutex, time::{sleep, timeout}};

#[derive(Clone, Debug, Serialize)]
pub struct ModelRole {
    pub role: &'static str,
    pub url: String,
    pub model: String,
    pub served_model_name: String,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum RoleKind {
    Planner,
    CodeGeneration,
    MathReasoning,
    Unknown,
}

impl RoleKind {
    pub fn from_role_name(role: &str) -> Self {
        match role {
            "planner" => RoleKind::Planner,
            "code" | "code_generation" => RoleKind::CodeGeneration,
            "math" | "math_reasoning" => RoleKind::MathReasoning,
            _ => RoleKind::Unknown,
        }
    }

    pub fn remote_model_env(self) -> &'static str {
        match self {
            RoleKind::Planner => "VERITAS_REMOTE_PLANNER_MODEL",
            RoleKind::CodeGeneration => "VERITAS_REMOTE_CODE_MODEL",
            RoleKind::MathReasoning => "VERITAS_REMOTE_MATH_MODEL",
            RoleKind::Unknown => "VERITAS_REMOTE_MODEL_NAME",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
pub enum ProviderType {
    LocalVllm,
    RemoteOpenAICompatible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ProviderFailureCategory {
    Transport,
    Timeout,
    ModelUnavailable,
    GpuOutOfMemory,
    ContextTooLong,
    RateLimited,
    AuthFailure,
    InvalidJson,
    SchemaViolation,
    CircuitOpen,
    Upstream,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderError {
    pub provider: ProviderType,
    pub role: String,
    pub category: ProviderFailureCategory,
    pub code: String,
    pub message: String,
    pub remediation: String,
    pub retryable: bool,
    pub details: Value,
}

impl ProviderError {
    fn new(provider: ProviderType, role: &str, category: ProviderFailureCategory, code: &str, message: impl Into<String>, remediation: impl Into<String>, retryable: bool) -> Self {
        Self {
            provider,
            role: role.to_string(),
            category,
            code: code.to_string(),
            message: message.into(),
            remediation: remediation.into(),
            retryable,
            details: json!({}),
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub role: ModelRole,
    pub system: String,
    pub user: String,
    pub schema: Option<SchemaKey>,
}

pub trait ModelProvider: Send + Sync {
    fn provider_type(&self) -> ProviderType;
    fn health<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>>;
    fn metadata<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>>;
    fn chat<'a>(&'a self, req: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct LocalVllmProvider {
    http: Client,
}

impl LocalVllmProvider {
    pub fn new(http: Client) -> Self { Self { http } }
}

#[derive(Clone)]
pub struct RemoteOpenAICompatibleProvider {
    http: Client,
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

impl RemoteOpenAICompatibleProvider {
    pub fn new(http: Client, base_url: String, model: String, api_key_env: String) -> Self {
        Self { http, base_url, model, api_key_env }
    }

    pub fn enabled(&self) -> bool {
        !self.base_url.trim().is_empty()
            && (!self.model.trim().is_empty()
                || env::var("VERITAS_REMOTE_PLANNER_MODEL").ok().filter(|v| !v.trim().is_empty()).is_some()
                || env::var("VERITAS_REMOTE_CODE_MODEL").ok().filter(|v| !v.trim().is_empty()).is_some()
                || env::var("VERITAS_REMOTE_MATH_MODEL").ok().filter(|v| !v.trim().is_empty()).is_some())
    }

    fn model_for_role(&self, role: &ModelRole) -> String {
        let role_kind = RoleKind::from_role_name(role.role);
        env::var(role_kind.remote_model_env())
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.model.clone())
    }
}

impl ModelProvider for LocalVllmProvider {
    fn provider_type(&self) -> ProviderType { ProviderType::LocalVllm }

    fn health<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move { openai_compatible_models_health(&self.http, ProviderType::LocalVllm, role, &role.url, &role.served_model_name, None).await })
    }

    fn metadata<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            Ok(json!({
                "provider": "local_vllm",
                "protocol": "openai_compatible",
                "role": role.role,
                "base_url": role.url,
                "model": role.model,
                "served_model_name": role.served_model_name,
                "timeout_secs": role.timeout_secs,
                "temperature": role.temperature,
                "top_p": role.top_p,
                "max_tokens": role.max_tokens
            }))
        })
    }

    fn chat<'a>(&'a self, req: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            send_openai_compatible_chat(&self.http, ProviderType::LocalVllm, &req.role.url, &req.role.served_model_name, &req.role, &req.system, &req.user, req.schema, None).await
        })
    }
}

impl ModelProvider for RemoteOpenAICompatibleProvider {
    fn provider_type(&self) -> ProviderType { ProviderType::RemoteOpenAICompatible }

    fn health<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.enabled() {
                return Err(ProviderError::new(ProviderType::RemoteOpenAICompatible, role.role, ProviderFailureCategory::ModelUnavailable, "remote.disabled", "Remote fallback is disabled or incomplete.", "Set VERITAS_REMOTE_MODEL_ENABLED=true with base URL, role-specific model, and API key env only when fallback is desired.", false));
            }
            let api_key = env::var(&self.api_key_env).ok().filter(|v| !v.trim().is_empty());
            openai_compatible_models_health(&self.http, ProviderType::RemoteOpenAICompatible, role, &self.base_url, &self.model_for_role(role), api_key).await
        })
    }

    fn metadata<'a>(&'a self, role: &'a ModelRole) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            Ok(json!({
                "provider": "remote_openai_compatible",
                "role": role.role,
                "model": self.model_for_role(role),
                "default_model": self.model,
                "base_url": self.base_url,
                "api_key_env": self.api_key_env,
                "privacy": "remote fallback may send planner/code/math context outside the local host; enable only with explicit operator consent"
            }))
        })
    }

    fn chat<'a>(&'a self, req: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.enabled() {
                return Err(ProviderError::new(ProviderType::RemoteOpenAICompatible, req.role.role, ProviderFailureCategory::ModelUnavailable, "remote.disabled", "Remote fallback is disabled or incomplete.", "Set VERITAS_REMOTE_MODEL_ENABLED=true with base URL, role-specific model, and API key env only when fallback is desired.", false));
            }
            let mut role = req.role.clone();
            role.url = self.base_url.clone();
            role.model = self.model_for_role(&req.role);
            role.served_model_name = role.model.clone();
            let api_key = env::var(&self.api_key_env).ok().filter(|v| !v.trim().is_empty());
            send_openai_compatible_chat(&self.http, ProviderType::RemoteOpenAICompatible, &self.base_url, &role.served_model_name, &role, &req.system, &req.user, req.schema, api_key).await
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderRetryPolicy {
    pub max_attempts: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub circuit_failure_threshold: u32,
    pub circuit_cooldown_secs: u64,
}

impl ProviderRetryPolicy {
    fn from_env() -> Self {
        Self {
            max_attempts: usize_env("VERITAS_PROVIDER_RETRY_MAX_ATTEMPTS", 3).max(1),
            base_delay_ms: u64_env("VERITAS_PROVIDER_RETRY_BASE_DELAY_MS", 150),
            max_delay_ms: u64_env("VERITAS_PROVIDER_RETRY_MAX_DELAY_MS", 2000),
            circuit_failure_threshold: u32_env("VERITAS_PROVIDER_CIRCUIT_FAILURE_THRESHOLD", 3).max(1),
            circuit_cooldown_secs: u64_env("VERITAS_PROVIDER_CIRCUIT_COOLDOWN_SECS", 30),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
enum CircuitStateKind {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone, Debug)]
struct CircuitRecord {
    state: CircuitStateKind,
    failures: u32,
    opened_at: Option<Instant>,
}

impl Default for CircuitRecord {
    fn default() -> Self { Self { state: CircuitStateKind::Closed, failures: 0, opened_at: None } }
}

#[derive(Clone)]
pub struct ProviderRouter {
    local: LocalVllmProvider,
    remote: RemoteOpenAICompatibleProvider,
    remote_enabled: bool,
    retry_policy: ProviderRetryPolicy,
    circuits: Arc<Mutex<HashMap<String, CircuitRecord>>>,
    route_history: Arc<Mutex<Vec<Value>>>,
}

impl ProviderRouter {
    pub fn new(http: Client, remote_enabled: bool, remote_base_url: String, remote_model: String, remote_api_key_env: String) -> Self {
        Self {
            local: LocalVllmProvider::new(http.clone()),
            remote: RemoteOpenAICompatibleProvider::new(http, remote_base_url, remote_model, remote_api_key_env),
            remote_enabled,
            retry_policy: ProviderRetryPolicy::from_env(),
            circuits: Arc::new(Mutex::new(HashMap::new())),
            route_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn chat_raw(&self, role: &ModelRole, system: &str, user: &str, schema: Option<SchemaKey>) -> Result<Value, ProviderError> {
        let request = ChatRequest { role: role.clone(), system: system.to_string(), user: user.to_string(), schema };
        let local_result = self.call_with_retries(&self.local, request.clone()).await;
        match local_result {
            Ok(value) => Ok(value),
            Err(local_error) => {
                if self.remote_enabled && self.remote.enabled() && local_error.retryable {
                    match self.call_with_retries(&self.remote, request).await {
                        Ok(value) => Ok(annotate_provider_route(value, role, "remote_openai_compatible", Some(local_error))),
                        Err(remote_error) => Err(ProviderError::new(
                            ProviderType::RemoteOpenAICompatible,
                            role.role,
                            ProviderFailureCategory::Upstream,
                            "model.all_providers_failed",
                            "Both local vLLM and configured remote OpenAI-compatible fallback failed.",
                            "Check local vLLM health, remote endpoint/API key, GPU memory, model IDs, and provider circuit breaker state.",
                            false,
                        ).with_details(json!({"local_error": local_error, "remote_error": remote_error}))),
                    }
                } else {
                    Err(local_error)
                }
            }
        }
    }

    async fn call_with_retries<P: ModelProvider>(&self, provider: &P, req: ChatRequest) -> Result<Value, ProviderError> {
        let provider_type = provider.provider_type();
        let key = circuit_key(provider_type, req.role.role);
        if let Some(error) = self.circuit_block_error(&key, provider_type, req.role.role).await {
            self.record_route_event(provider_type, req.role.role, "circuit_open", 0, Some(&error), None).await;
            return Err(error);
        }

        let mut last_error: Option<ProviderError> = None;
        for attempt in 1..=self.retry_policy.max_attempts {
            let result = provider.chat(req.clone()).await;
            match result {
                Ok(value) => {
                    self.record_success(&key).await;
                    self.record_route_event(provider_type, req.role.role, "success", attempt, None, Some(&value)).await;
                    return Ok(value);
                }
                Err(error) => {
                    let retryable = error.retryable && attempt < self.retry_policy.max_attempts;
                    self.record_route_event(provider_type, req.role.role, if retryable { "retryable_error" } else { "terminal_error" }, attempt, Some(&error), None).await;
                    last_error = Some(error.clone());
                    if !retryable {
                        self.record_failure(&key).await;
                        let details = error.details.clone();
                        return Err(error.with_details(merge_details(details, json!({"attempt": attempt, "max_attempts": self.retry_policy.max_attempts}))));
                    }
                    sleep(self.retry_delay(attempt)).await;
                }
            }
        }
        let error = last_error.unwrap_or_else(|| ProviderError::new(provider_type, req.role.role, ProviderFailureCategory::Upstream, "model.retry_exhausted", "Model provider retry budget was exhausted without a successful response.", "Check provider logs, model health, retry policy, and circuit breaker state.", false));
        self.record_failure(&key).await;
        Err(error)
    }

    pub async fn health_for_role(&self, role: &ModelRole) -> Value {
        let local = self.local.health(role).await;
        let remote = if self.remote_enabled && self.remote.enabled() { Some(self.remote.health(role).await) } else { None };
        json!({
            "role": role.role,
            "local_vllm": result_to_json(local),
            "remote_openai_compatible": remote.map(result_to_json).unwrap_or_else(|| json!({"ok": false, "status": "disabled"})),
            "circuit": self.circuit_snapshot_for_role(role.role).await,
        })
    }

    pub async fn history_snapshot(&self) -> Vec<Value> {
        self.route_history.lock().await.clone()
    }

    pub async fn circuit_snapshot_for_role(&self, role: &str) -> Value {
        let circuits = self.circuits.lock().await;
        let mut out = serde_json::Map::new();
        for provider_type in [ProviderType::LocalVllm, ProviderType::RemoteOpenAICompatible] {
            let key = circuit_key(provider_type, role);
            if let Some(record) = circuits.get(&key) {
                let remaining = record.opened_at
                    .and_then(|opened| self.retry_policy.circuit_cooldown_secs.checked_sub(opened.elapsed().as_secs()))
                    .unwrap_or(0);
                out.insert(format!("{:?}", provider_type), json!({"state": format!("{:?}", record.state), "failures": record.failures, "cooldown_remaining_secs": remaining}));
            } else {
                out.insert(format!("{:?}", provider_type), json!({"state": "Closed", "failures": 0, "cooldown_remaining_secs": 0}));
            }
        }
        Value::Object(out)
    }

    pub fn summary(&self) -> Value {
        json!({
            "local_provider": "vllm",
            "remote_fallback_enabled": self.remote_enabled,
            "remote_fallback_configured": self.remote.enabled(),
            "remote_model": self.remote.model,
            "remote_base_url": self.remote.base_url,
            "remote_api_key_env": self.remote.api_key_env,
            "role_specific_remote_model_env": {"planner":"VERITAS_REMOTE_PLANNER_MODEL","code":"VERITAS_REMOTE_CODE_MODEL","math":"VERITAS_REMOTE_MATH_MODEL"},
            "per_role_remote_model_env": {"planner":"VERITAS_REMOTE_PLANNER_MODEL", "code_generation":"VERITAS_REMOTE_CODE_MODEL", "math_reasoning":"VERITAS_REMOTE_MATH_MODEL"},
            "retry_policy": self.retry_policy,
            "circuit_breaker": {"enabled": true, "failure_threshold": self.retry_policy.circuit_failure_threshold, "cooldown_secs": self.retry_policy.circuit_cooldown_secs},
            "privacy": "remote fallback is opt-in and must be enabled explicitly by configuration"
        })
    }

    async fn circuit_block_error(&self, key: &str, provider_type: ProviderType, role: &str) -> Option<ProviderError> {
        let mut circuits = self.circuits.lock().await;
        let record = circuits.entry(key.to_string()).or_default();
        match record.state {
            CircuitStateKind::Closed => None,
            CircuitStateKind::HalfOpen => None,
            CircuitStateKind::Open => {
                let elapsed = record.opened_at.map(|opened| opened.elapsed()).unwrap_or_default();
                if elapsed >= Duration::from_secs(self.retry_policy.circuit_cooldown_secs) {
                    record.state = CircuitStateKind::HalfOpen;
                    None
                } else {
                    Some(ProviderError::new(provider_type, role, ProviderFailureCategory::CircuitOpen, "model.circuit_open", format!("Provider circuit is open for {role}; cooldown has not elapsed."), "Wait for cooldown, check model service health, or use an explicitly configured fallback provider.", true).with_details(json!({"cooldown_remaining_secs": self.retry_policy.circuit_cooldown_secs.saturating_sub(elapsed.as_secs())})))
                }
            }
        }
    }

    async fn record_success(&self, key: &str) {
        let mut circuits = self.circuits.lock().await;
        circuits.insert(key.to_string(), CircuitRecord::default());
    }

    async fn record_failure(&self, key: &str) {
        let mut circuits = self.circuits.lock().await;
        let record = circuits.entry(key.to_string()).or_default();
        record.failures = record.failures.saturating_add(1);
        if record.failures >= self.retry_policy.circuit_failure_threshold {
            record.state = CircuitStateKind::Open;
            record.opened_at = Some(Instant::now());
        }
    }

    async fn record_route_event(&self, provider_type: ProviderType, role: &str, outcome: &str, attempt: usize, error: Option<&ProviderError>, value: Option<&Value>) {
        let mut history = self.route_history.lock().await;
        history.push(json!({
            "provider": format!("{:?}", provider_type),
            "role": role,
            "outcome": outcome,
            "attempt": attempt,
            "error": error,
            "response_id": value.and_then(|v| v.get("id")).cloned().unwrap_or(Value::Null),
            "timestamp_ms": unix_ms()
        }));
        if history.len() > 200 {
            let excess = history.len() - 200;
            history.drain(0..excess);
        }
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let exp = 1_u64.checked_shl((attempt.saturating_sub(1)).min(10) as u32).unwrap_or(1);
        let base = self.retry_policy.base_delay_ms.saturating_mul(exp);
        let jitter = (unix_ms() % 97) as u64;
        Duration::from_millis(base.saturating_add(jitter).min(self.retry_policy.max_delay_ms))
    }
}

fn annotate_provider_route(mut value: Value, role: &ModelRole, provider: &str, local_error: Option<ProviderError>) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("veritas_provider_route".to_string(), json!({
            "role": role.role,
            "provider": provider,
            "served_model_name": role.served_model_name,
            "local_error_before_fallback": local_error
        }));
    }
    value
}

async fn openai_compatible_models_health(
    http: &Client,
    provider: ProviderType,
    role: &ModelRole,
    base_url: &str,
    expected_model: &str,
    api_key: Option<String>,
) -> Result<Value, ProviderError> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let started = Instant::now();
    let mut request = http.get(&url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(Duration::from_secs(role.timeout_secs), request.send()).await
        .map_err(|_| ProviderError::new(provider, role.role, ProviderFailureCategory::Timeout, "model.health_timeout", format!("Model health request for role {} timed out after {}s", role.role, role.timeout_secs), "Start the vLLM/OpenAI-compatible service, reduce load, or increase timeout.", true))?
        .map_err(|error| ProviderError::new(provider, role.role, ProviderFailureCategory::Transport, "model.health_transport", format!("Model health request failed before response: {error}"), "Check provider URL, network, service logs, and Docker health.", true))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        let category = classify_http_error(status.as_u16(), &body);
        return Err(ProviderError::new(provider, role.role, category, "model.health_upstream", format!("Model health endpoint returned HTTP {}", status.as_u16()), "Check /v1/models on the configured provider endpoint.", true).with_details(body));
    }
    let model_ids: Vec<String> = body.get("data").and_then(Value::as_array).map(|items| {
        items.iter().filter_map(|item| item.get("id").and_then(Value::as_str).map(ToString::to_string)).collect()
    }).unwrap_or_default();
    let found = model_ids.iter().any(|id| id == expected_model || id == &role.served_model_name || id == &role.model);
    if !found {
        return Err(ProviderError::new(provider, role.role, ProviderFailureCategory::ModelUnavailable, "model.health_model_missing", format!("Provider is reachable, but expected model `{expected_model}` was not listed by /v1/models."), "Check served-model-name, model ID, and vLLM startup flags.", false).with_details(json!({"models": model_ids, "body": body})));
    }
    Ok(json!({
        "ok": true,
        "provider": format!("{:?}", provider),
        "role": role.role,
        "base_url": base_url,
        "expected_model": expected_model,
        "models": model_ids,
        "latency_ms": started.elapsed().as_millis(),
        "checked_endpoint": url
    }))
}

async fn send_openai_compatible_chat(
    http: &Client,
    provider: ProviderType,
    base_url: &str,
    served_model_name: &str,
    role: &ModelRole,
    system: &str,
    user: &str,
    schema: Option<SchemaKey>,
    api_key: Option<String>,
) -> Result<Value, ProviderError> {
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let mut payload = json!({
        "model": served_model_name,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        "temperature": role.temperature,
        "top_p": role.top_p,
        "max_tokens": role.max_tokens
    });
    if let Some(schema_key) = schema {
        payload["response_format"] = json!({"type": "json_object"});
        payload["guided_json"] = schema_json(schema_key);
        payload["extra_body"] = json!({"guided_json": schema_json(schema_key)});
    }
    let mut request = http.post(&url).json(&payload);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(Duration::from_secs(role.timeout_secs), request.send()).await
        .map_err(|_| ProviderError::new(provider, role.role, ProviderFailureCategory::Timeout, "model.timeout", format!("Model request for role {} timed out after {}s", role.role, role.timeout_secs), "Increase the role timeout, reduce context length, or choose a smaller/faster model.", true))?
        .map_err(|error| ProviderError::new(provider, role.role, ProviderFailureCategory::Transport, "model.transport", format!("Model request failed before response: {error}"), "Start the selected vLLM service or configure a reachable OpenAI-compatible fallback.", true))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        let category = classify_http_error(status.as_u16(), &body);
        let retryable = matches!(category, ProviderFailureCategory::RateLimited | ProviderFailureCategory::ModelUnavailable | ProviderFailureCategory::GpuOutOfMemory | ProviderFailureCategory::ContextTooLong | ProviderFailureCategory::Upstream | ProviderFailureCategory::Timeout | ProviderFailureCategory::Transport);
        return Err(ProviderError::new(provider, role.role, category, "model.upstream", format!("Model endpoint returned HTTP {}", status.as_u16()), "Check model ID, Hugging Face token, GPU memory, provider URL, and logs.", retryable).with_details(body));
    }
    Ok(annotate_provider_route(body, role, match provider { ProviderType::LocalVllm => "local_vllm", ProviderType::RemoteOpenAICompatible => "remote_openai_compatible" }, None))
}

fn classify_http_error(status: u16, body: &Value) -> ProviderFailureCategory {
    let text = body.to_string().to_ascii_lowercase();
    if status == 401 || status == 403 { ProviderFailureCategory::AuthFailure }
    else if status == 408 || status == 504 { ProviderFailureCategory::Timeout }
    else if status == 429 { ProviderFailureCategory::RateLimited }
    else if text.contains("out of memory") || (text.contains("cuda") && text.contains("memory")) { ProviderFailureCategory::GpuOutOfMemory }
    else if text.contains("context") && (text.contains("length") || text.contains("long")) { ProviderFailureCategory::ContextTooLong }
    else if status == 404 { ProviderFailureCategory::ModelUnavailable }
    else { ProviderFailureCategory::Upstream }
}

fn circuit_key(provider: ProviderType, role: &str) -> String {
    format!("{:?}:{role}", provider)
}

fn result_to_json(result: Result<Value, ProviderError>) -> Value {
    match result {
        Ok(value) => json!({"ok": true, "status": "healthy", "details": value}),
        Err(error) => json!({"ok": false, "status": "unhealthy", "error": error}),
    }
}

fn merge_details(existing: Value, extra: Value) -> Value {
    let mut merged = serde_json::Map::new();
    merged.insert("existing".to_string(), existing);
    merged.insert("extra".to_string(), extra);
    Value::Object(merged)
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn usize_env(name: &str, default: usize) -> usize {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn u64_env(name: &str, default: u64) -> u64 {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn u32_env(name: &str, default: u32) -> u32 {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
