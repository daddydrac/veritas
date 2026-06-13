# Phase 7 — Artifact Decision Engine

Phase 7 makes artifact status a canonical application decision instead of a status string assigned by the code-generation loop.

## Business rule

No Veritas report may imply production readiness unless every required upstream gate passed:

- evidence eligibility
- formula/citation review
- representation readiness
- math tool validation
- human checkpoints
- pre-codegen SHACL
- compile/test validation
- final SHACL
- host validation when production validation is claimed

## Application behavior

`apps/api/src/artifact_decision.rs` computes `artifact_decision.json` from real run artifacts:

- `gate_decisions.jsonl`
- `pre_codegen_gate_report.json`
- `validation_results.json`
- `commands_run.json`
- `retry_history.json`
- `human_checkpoints.jsonl`
- `final_artifact_shacl_report.json`
- `host_validation_summary.jsonl` or `host_validation_summary.json`

The final report now copies the final status from `artifact_decision.artifact_status`. The code-generation loop can report validation results, but it cannot independently grant production status.

## Key statuses

| Status | Meaning |
|---|---|
| `awaiting_evidence_review` | Evidence, formulas, or citations still require review. |
| `awaiting_human_approval` | A required human checkpoint has not approved or waived progression. |
| `blocked_by_formula_review` | A formula is rejected, pending, low confidence, or otherwise not eligible. |
| `blocked_by_citation_review` | A citation is rejected, pending, or not usable for audit. |
| `blocked_by_math_tools` | Tool-verified math validation failed or is missing for a math-heavy run. |
| `blocked_by_governance` | SHACL or governance checks blocked execution or final status. |
| `validation_failed` | Validation commands failed and no successful repair produced a pass. |
| `repair_failed` | Repair attempts were made but validation still did not pass. |
| `local_validated_host_pending` | Application gates and local validation passed, but host production validation is pending. |
| `production_candidate_validated` | Application gates and validation passed and candidate-with-host-pending policy explicitly allows this label. |
| `production_validated` | Application gates, validation, governance, and host validation passed. |

## Host validation rule

`production_validated` is impossible unless host validation evidence exists and passes. Without host validation evidence, the strongest default status is `local_validated_host_pending`.

## Files produced

Every completed or blocked run now writes:

```text
artifact_decision.json
final_report.json
```

Pre-codegen blocked runs also write:

```text
pre_codegen_blocked_report.json
```

## Verification

Run:

```bash
PYTHONPATH=services/ingestion PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 \
  pytest -q tests/ingestion/test_phase7_artifact_decision_engine.py --disable-warnings

python3 scripts/validate-spec.py
```
