use jsonschema::{Draft, JSONSchema};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SchemaKey {
    Planner,
    Codegen,
    MathReasoning,
    Repair,
    HumanCheckpoint,
    RunReport,
}

impl SchemaKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaKey::Planner => "planner",
            SchemaKey::Codegen => "codegen",
            SchemaKey::MathReasoning => "math_reasoning",
            SchemaKey::Repair => "repair",
            SchemaKey::HumanCheckpoint => "human_checkpoint",
            SchemaKey::RunReport => "run_report",
        }
    }
}

pub fn schema_json(key: SchemaKey) -> Value {
    let raw = match key {
        SchemaKey::Planner => include_str!("../../../schemas/planner.schema.json"),
        SchemaKey::Codegen => include_str!("../../../schemas/codegen.schema.json"),
        SchemaKey::MathReasoning => include_str!("../../../schemas/math_reasoning.schema.json"),
        SchemaKey::Repair => include_str!("../../../schemas/repair.schema.json"),
        SchemaKey::HumanCheckpoint => include_str!("../../../schemas/human_checkpoint.schema.json"),
        SchemaKey::RunReport => include_str!("../../../schemas/run_report.schema.json"),
    };
    serde_json::from_str(raw).unwrap_or_else(|_| json!({"type":"object"}))
}

pub fn schema_required_fields(key: SchemaKey) -> Vec<String> {
    schema_json(key)
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// Validate a JSON value against the complete role-specific JSON Schema.
///
/// The same schema object is sent to vLLM as `guided_json`, so this is the
/// application-side trust boundary that prevents malformed LLM output from
/// writing files, running commands, updating graphs, or changing artifact status.
pub fn validate_json_schema(key: SchemaKey, value: &Value) -> Result<(), Vec<String>> {
    let schema = schema_json(key);
    let compiled = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema)
        .map_err(|error| vec![format!("{} schema could not be compiled: {error}", key.as_str())])?;

    match compiled.validate(value) {
        Ok(()) => Ok(()),
        Err(errors) => Err(errors
            .map(|error| {
                let path = error.instance_path.to_string();
                if path.is_empty() || path == "/" {
                    error.to_string()
                } else {
                    format!("{path}: {error}")
                }
            })
            .collect()),
    }
}

/// Backward-compatible alias retained for older call sites and tests.
/// It now runs full recursive JSON Schema validation.
#[allow(dead_code)]
pub fn validate_required_object_fields(key: SchemaKey, value: &Value) -> Result<(), Vec<String>> {
    validate_json_schema(key, value)
}
