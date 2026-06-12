use crate::evidence_registry;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) async fn evaluate(workspace: &Path) -> Value {
    let registry_path = workspace.join("evidence_registry.json");
    let source_manifest_path = workspace.join("source_manifest.json");
    let evidence_manifest_path = workspace.join("evidence_manifest.json");
    let source_manifest = crate::read_json_file(&source_manifest_path).await;
    let source_requires_evidence = source_manifest.as_ref().and_then(|v| v.get("source")).map(|v| !v.is_null()).unwrap_or(false)
        || evidence_manifest_path.exists()
        || workspace.join("formula_manifest.json").exists()
        || workspace.join("citation_manifest.json").exists();

    if !registry_path.exists() {
        return json!({
            "ok": !source_requires_evidence,
            "enforced": source_requires_evidence,
            "blocking": source_requires_evidence,
            "gate": "evidence",
            "stage": "evidence_eligibility",
            "status": if source_requires_evidence { "awaiting_evidence_review" } else { "not_applicable_prompt_only" },
            "reason": if source_requires_evidence { "evidence_registry.json is required because source/evidence artifacts exist." } else { "No source evidence artifacts were supplied for this prompt-only run." },
            "registry_path": registry_path.display().to_string(),
            "next_action": if source_requires_evidence { "Review citations/formulas and rebuild evidence_registry.json before planning/codegen." } else { "Continue; no source-bound evidence gate applies." },
            "files_written_allowed": !source_requires_evidence,
            "commands_run_allowed": !source_requires_evidence,
        });
    }

    match evidence_registry::planning_gate_from_workspace(workspace).await {
        Ok(gate) => {
            let ok = gate.get("ok").and_then(Value::as_bool).unwrap_or(false);
            json!({
                "ok": ok,
                "enforced": true,
                "blocking": !ok,
                "gate": "evidence",
                "stage": "evidence_eligibility",
                "status": if ok { "eligible_for_evidence_backed_planning" } else { "awaiting_evidence_review" },
                "decision": gate,
                "files_written_allowed": ok,
                "commands_run_allowed": ok,
            })
        }
        Err(error) => json!({
            "ok": false,
            "enforced": true,
            "blocking": true,
            "gate": "evidence",
            "stage": "evidence_eligibility",
            "status": "awaiting_evidence_review",
            "error": {"code": error.code, "message": error.message, "remediation": error.remediation, "details": error.details},
            "files_written_allowed": false,
            "commands_run_allowed": false,
        }),
    }
}
