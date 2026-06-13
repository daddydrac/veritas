# PHASE6_SHACL_ARTIFACT_GOVERNANCE

Phase 6 makes SHACL governance default-on and artifact-based in the real application path.

## Behavior

Veritas now uses `VERITAS_GOVERNANCE_MODE` instead of treating SHACL as a simple advisory boolean.

Supported modes:

- `enforce`: SHACL findings block governed execution.
- `advisory`: SHACL findings are recorded but exploratory execution may continue.
- `disabled`: SHACL is skipped and the run cannot claim production validation.

Default behavior is `enforce` for local, journey, and production-like profiles. Development profiles may explicitly use advisory mode.

## Artifact bundle

SHACL data is built from the real run workspace, including:

- `evidence_manifest.json`
- `formula_manifest.json`
- `citation_manifest.json`
- `evidence_registry.json`
- `evidence_eligibility.json`
- `representation_model.json`
- `planning_context.json`
- `plan.json`
- `code_package_latest.json`
- `validation_results.json`
- `human_checkpoints.jsonl`
- `math_validation_report.json`
- `math_tool_results.jsonl`
- `gate_decisions.jsonl`

The artifact list is configurable through `VERITAS_SHACL_ARTIFACT_FILES`.

## Runtime gates

SHACL now runs before code generation and again after validation. In enforce mode, a failing pre-codegen SHACL gate blocks before files are written or commands are run. A failing final SHACL gate changes final status to `blocked_by_governance`.
