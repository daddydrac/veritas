# Phase 8 — Strengthened Lineage Schemas

Phase 8 makes lineage part of the real application trust boundary.  Planner output, code generation output, and final reports are no longer accepted merely because they are structurally valid JSON. They must carry explicit references to the evidence, citations, formulas, risks, validation gates, human checkpoints, and artifact decisions that justify downstream execution.

## Production behavior

The application now builds a `planning_context.json` from real workspace artifacts and validates planner/codegen references against that context. Code generation is rejected before any file write when generated files or validation commands do not cite approved plan, evidence, citation, formula, and validation identifiers.

## Files affected

- `schemas/planner.schema.json`
- `schemas/codegen.schema.json`
- `schemas/run_report.schema.json`
- `apps/api/src/lineage.rs`
- `apps/api/src/main.rs`

## Enforcement outcome

A generated file cannot be written unless it includes:

```text
path
content
purpose
derived_from_plan_step_ids
derived_from_evidence_ids
derived_from_citation_ids
derived_from_formula_ids
required_validation_ids
```

The final report must include source documents, citations, formulas, review decisions, representation model, planning context, plan lineage, file lineage, command lineage, validation lineage, repair lineage, governance lineage, artifact decision, and final status.

## Acceptance check

Run:

```bash
PYTHONPATH=services/ingestion PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 \
pytest -q tests/ingestion/test_phase8_lineage_schemas.py --disable-warnings
```
