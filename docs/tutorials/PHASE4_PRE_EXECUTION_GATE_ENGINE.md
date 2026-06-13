# Phase 4 — Pre-Execution Gate Engine

Phase 4 moves governance from report-time metadata into the real application control flow. The product path now runs a pre-codegen gate report before generated files are written or validation commands are executed.

## What is enforced

The Gate Engine evaluates real run artifacts in the workspace:

- `evidence_registry.json` for evidence-backed planning eligibility.
- `human_checkpoints.jsonl` for required `plan_review` and `code_architecture_review` decisions.
- `representation_model.json` for math-heavy representation readiness.
- `math_validation_report.json` for math-heavy tool-verified math readiness.
- `automatic_shacl_report.json` for pre-codegen SHACL governance.

If any enforced gate fails, Veritas writes:

- `gate_decisions.jsonl`
- `pre_codegen_gate_report.json`
- `pre_codegen_blocked_report.json`
- `final_report.json`

and returns without generating code, writing files, or running validation commands.

## Required human checkpoints

By default, the pre-codegen workflow requires:

```text
plan_review
code_architecture_review
```

Operators can configure the list with:

```bash
VERITAS_PRE_CODEGEN_CHECKPOINTS=plan_review,code_architecture_review
```

Each required checkpoint must be approved, edited, or skipped with an explicit waiver reason. Rejections and missing approvals block the workflow.

## Expected blocked state

When approval is missing, the application returns an auditable blocked report:

```json
{
  "ok": false,
  "final_status": "awaiting_human_approval",
  "blocked_stage": "plan_review",
  "files_changed": [],
  "commands_run": []
}
```

## Why this matters

The earlier implementation recorded human checkpoints after code generation and validation. Phase 4 changes that behavior: approval gates now causally block codegen before any generated files or commands can exist.

## Phase 5 handoff

Phase 5 adds the Tool-Verified Math Engine that produces `math_validation_report.json` before these gates evaluate math-heavy runs.
