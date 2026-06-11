use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SchemaKey {
    Planner,
    Codegen,
    MathReasoning,
    Repair,
    RunReport,
}

impl SchemaKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaKey::Planner => "planner",
            SchemaKey::Codegen => "codegen",
            SchemaKey::MathReasoning => "math_reasoning",
            SchemaKey::Repair => "repair",
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

pub fn validate_required_object_fields(key: SchemaKey, value: &Value) -> Result<(), Vec<String>> {
    if !value.is_object() {
        return Err(vec![format!("{} output must be a JSON object", key.as_str())]);
    }
    let mut errors = Vec::new();
    for field in schema_required_fields(key) {
        if value.get(&field).is_none() {
            errors.push(format!("required field `{field}` is missing"));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
