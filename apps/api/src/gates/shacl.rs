use serde_json::{json, Value};

pub(crate) fn evaluate_pre_codegen(automatic_shacl: &Value, shacl_enforced: bool) -> Value {
    let conforms = automatic_shacl.get("conforms").and_then(Value::as_bool)
        .or_else(|| automatic_shacl.pointer("/result/conforms").and_then(Value::as_bool))
        .unwrap_or(true);
    let shacl_ok = automatic_shacl.get("ok").and_then(Value::as_bool).unwrap_or(true);
    let ok = shacl_ok && conforms;
    json!({
        "ok": ok || !shacl_enforced,
        "conforms": conforms,
        "enforced": shacl_enforced,
        "blocking": shacl_enforced && !ok,
        "gate": "shacl",
        "stage": "pre_codegen_shacl",
        "status": if ok { "shacl_conforms" } else if shacl_enforced { "blocked_by_governance" } else { "advisory_shacl_nonconformance" },
        "automatic_shacl": automatic_shacl,
        "files_written_allowed": ok || !shacl_enforced,
        "commands_run_allowed": ok || !shacl_enforced,
    })
}
