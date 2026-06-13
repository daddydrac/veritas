use serde_json::Value;

use super::executor::ToolExecutionRequest;

pub(crate) fn ordered_math_tool_requests(formula_payload: &Value, tool_names: &[String]) -> Vec<ToolExecutionRequest> {
    tool_names.iter().map(|name| ToolExecutionRequest::new(name.clone(), formula_payload.clone())).collect()
}
