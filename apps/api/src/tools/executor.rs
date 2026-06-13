use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct ToolExecutionRequest {
    pub(crate) tool_name: String,
    pub(crate) payload: Value,
}

impl ToolExecutionRequest {
    pub(crate) fn new(tool_name: impl Into<String>, payload: Value) -> Self {
        Self { tool_name: tool_name.into(), payload }
    }
}
