use serde_json::{json, Value};

pub(crate) fn vllm_tool_definitions() -> Value {
    json!([
        function_tool("parse_latex", "Parse LaTeX or expression text into normalized symbolic structure"),
        function_tool("normalize_expression", "Normalize a symbolic expression"),
        function_tool("symbolic_simplify", "Simplify expression or equation residual"),
        function_tool("numeric_validate", "Numerically validate formula over configured samples"),
        function_tool("counterexample_search", "Search sampled counterexamples"),
        function_tool("generate_property_tests", "Generate property tests from a symbolic expression")
    ])
}

fn function_tool(name: &str, description: &str) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": {
                    "latex": {"type": "string"},
                    "expression": {"type": "string"},
                    "assumptions": {"type": "array", "items": {"type": "string"}},
                    "variables": {"type": "array", "items": {"type": "string"}},
                    "metadata": {"type": "object"}
                }
            }
        }
    })
}
