use crate::governance::GovernanceMode;
use serde_json::{json, Value};

pub(crate) fn evaluate_pre_codegen(automatic_shacl: &Value, governance_mode: &GovernanceMode) -> Value {
    let conforms = automatic_shacl.get("conforms").and_then(Value::as_bool)
        .or_else(|| automatic_shacl.pointer("/result/conforms").and_then(Value::as_bool))
        .unwrap_or(true);
    let shacl_ok = automatic_shacl.get("ok").and_then(Value::as_bool).unwrap_or(true);
    let ok = shacl_ok && conforms;
    json!({
        "ok": ok || !governance_mode.enforces(),
        "conforms": conforms,
        "governance_mode": governance_mode.as_str(),
        "enforced": governance_mode.enforces(),
        "blocking": governance_mode.enforces() && !ok,
        "gate": "shacl",
        "stage": "pre_codegen_shacl",
        "status": if ok { "shacl_conforms" } else if governance_mode.enforces() { "blocked_by_governance" } else if governance_mode.disabled() { "disabled_shacl_nonconformance" } else { "advisory_shacl_nonconformance" },
        "automatic_shacl": automatic_shacl,
        "files_written_allowed": ok || !governance_mode.enforces(),
        "commands_run_allowed": ok || !governance_mode.enforces(),
    })
}
