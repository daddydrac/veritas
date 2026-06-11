use crate::schemas::{schema_json, SchemaKey};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::{env, future::Future, pin::Pin, time::Duration};
use tokio::time::timeout;

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
}

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ProviderType {
    LocalVllm,
    RemoteOpenAICompatible,
}

#[derive(Clone, Copy, Debug, Serialize)]
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
    fn health<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>>;
    fn metadata<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>>;
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
    base_url: String,
    model: String,
    api_key_env: String,
}

impl RemoteOpenAICompatibleProvider {
    pub fn new(http: Client, base_url: String, model: String, api_key_env: String) -> Self {
        Self { http, base_url, model, api_key_env }
    }

    pub fn enabled(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }
}

impl ModelProvider for LocalVllmProvider {
    fn provider_type(&self) -> ProviderType { ProviderType::LocalVllm }

    fn health<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async { Ok(json!({"provider":"local_vllm","status":"configured"})) })
    }

    fn metadata<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async { Ok(json!({"provider":"local_vllm","protocol":"openai_compatible"})) })
    }

    fn chat<'a>(&'a self, req: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            send_openai_compatible_chat(&self.http, ProviderType::LocalVllm, &req.role.url, &req.role.served_model_name, &req.role, &req.system, &req.user, req.schema, None).await
        })
    }
}

impl ModelProvider for RemoteOpenAICompatibleProvider {
    fn provider_type(&self) -> ProviderType { ProviderType::RemoteOpenAICompatible }

    fn health<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            if self.enabled() {
                Ok(json!({"provider":"remote_openai_compatible","status":"configured","base_url":self.base_url,"model":self.model}))
            } else {
                Err(ProviderError::new(ProviderType::RemoteOpenAICompatible, "remote", ProviderFailureCategory::ModelUnavailable, "remote.disabled", "Remote fallback is disabled or incomplete.", "Set VERITAS_REMOTE_MODEL_ENABLED=true with base URL, model, and API key env only when fallback is desired.", false))
            }
        })
    }

    fn metadata<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move { Ok(json!({"provider":"remote_openai_compatible","model":self.model,"base_url":self.base_url,"api_key_env":self.api_key_env})) })
    }

    fn chat<'a>(&'a self, req: ChatRequest) -> Pin<Box<dyn Future<Output = Result<Value, ProviderError>> + Send + 'a>> {
        Box::pin(async move {
            let mut role = req.role.clone();
            role.url = self.base_url.clone();
            role.model = self.model.clone();
            role.served_model_name = self.model.clone();
            let api_key = env::var(&self.api_key_env).ok().filter(|v| !v.trim().is_empty());
            send_openai_compatible_chat(&self.http, ProviderType::RemoteOpenAICompatible, &self.base_url, &self.model, &role, &req.system, &req.user, req.schema, api_key).await
        })
    }
}

#[derive(Clone)]
pub struct ProviderRouter {
    local: LocalVllmProvider,
    remote: RemoteOpenAICompatibleProvider,
    remote_enabled: bool,
}

impl ProviderRouter {
    pub fn new(http: Client, remote_enabled: bool, remote_base_url: String, remote_model: String, remote_api_key_env: String) -> Self {
        Self {
            local: LocalVllmProvider::new(http.clone()),
            remote: RemoteOpenAICompatibleProvider::new(http, remote_base_url, remote_model, remote_api_key_env),
            remote_enabled,
        }
    }

    pub async fn chat_raw(&self, role: &ModelRole, system: &str, user: &str, schema: Option<SchemaKey>) -> Result<Value, ProviderError> {
        let _role_kind = RoleKind::from_role_name(role.role);
        let request = ChatRequest { role: role.clone(), system: system.to_string(), user: user.to_string(), schema };
        let local_result = self.local.chat(request.clone()).await;
        match local_result {
            Ok(value) => Ok(value),
            Err(local_error) => {
                if self.remote_enabled && self.remote.enabled() && local_error.retryable {
                    match self.remote.chat(request).await {
                        Ok(value) => Ok(annotate_provider_route(value, role, "remote_openai_compatible", Some(local_error))),
                        Err(remote_error) => Err(ProviderError::new(
                            ProviderType::RemoteOpenAICompatible,
                            role.role,
                            ProviderFailureCategory::Upstream,
                            "model.all_providers_failed",
                            "Both local vLLM and configured remote OpenAI-compatible fallback failed.",
                            "Check local vLLM health, remote endpoint/API key, GPU memory, and model IDs.",
                            false,
                        ).with_details(json!({"local_error": local_error, "remote_error": remote_error}))),
                    }
                } else {
                    Err(local_error)
                }
            }
        }
    }

    pub fn summary(&self) -> Value {
        json!({
            "local_provider": "vllm",
            "remote_fallback_enabled": self.remote_enabled,
            "remote_fallback_configured": self.remote.enabled(),
            "remote_model": self.remote.model,
            "remote_base_url": self.remote.base_url,
            "remote_api_key_env": self.remote.api_key_env
        })
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
    }
    let mut request = http.post(&url).json(&payload);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(Duration::from_secs(role.timeout_secs), request.send()).await
        .map_err(|_| ProviderError::new(provider.clone(), role.role, ProviderFailureCategory::Timeout, "model.timeout", format!("Model request for role {} timed out after {}s", role.role, role.timeout_secs), "Increase the role timeout, reduce context length, or choose a smaller/faster model.", true))?
        .map_err(|error| ProviderError::new(provider.clone(), role.role, ProviderFailureCategory::Transport, "model.transport", format!("Model request failed before response: {error}"), "Start the selected vLLM service or configure a reachable OpenAI-compatible fallback.", true))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"raw": text}));
    if !status.is_success() {
        let category = classify_http_error(status.as_u16(), &body);
        let retryable = matches!(category, ProviderFailureCategory::RateLimited | ProviderFailureCategory::ModelUnavailable | ProviderFailureCategory::GpuOutOfMemory | ProviderFailureCategory::ContextTooLong | ProviderFailureCategory::Upstream);
        return Err(ProviderError::new(provider, role.role, category, "model.upstream", format!("Model endpoint returned HTTP {}", status.as_u16()), "Check model ID, Hugging Face token, GPU memory, provider URL, and logs.", retryable).with_details(body));
    }
    Ok(annotate_provider_route(body, role, match provider { ProviderType::LocalVllm => "local_vllm", ProviderType::RemoteOpenAICompatible => "remote_openai_compatible" }, None))
}

fn classify_http_error(status: u16, body: &Value) -> ProviderFailureCategory {
    let text = body.to_string().to_ascii_lowercase();
    if status == 401 || status == 403 { ProviderFailureCategory::AuthFailure }
    else if status == 408 || status == 504 { ProviderFailureCategory::Timeout }
    else if status == 429 { ProviderFailureCategory::RateLimited }
    else if text.contains("out of memory") || text.contains("cuda") && text.contains("memory") { ProviderFailureCategory::GpuOutOfMemory }
    else if text.contains("context") && (text.contains("length") || text.contains("long")) { ProviderFailureCategory::ContextTooLong }
    else if status == 404 { ProviderFailureCategory::ModelUnavailable }
    else { ProviderFailureCategory::Upstream }
}
