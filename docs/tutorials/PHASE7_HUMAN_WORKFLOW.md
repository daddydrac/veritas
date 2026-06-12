# Phase 7 — Human Review UX Across the Full Workflow

Phase 7 completes the source/mocked human-in-the-loop workflow for Veritas.
The review chain now covers:

```text
citation_review
→ formula_review
→ representation_review
→ plan_review
→ code_architecture_review
→ validation_review
```

The goal is to make researcher + machine teaming explicit.  A formula is not
allowed to become production code merely because it appears in a PDF or because
an LLM can generate code from it.  A human can approve, edit, reject, skip with a
reason, auto-approve under policy, or ask for explanation at each checkpoint.

## Policy modes

- `auto_approve`: checkpoints are recorded but do not block unless rejected.
- `require_all`: every checkpoint phase must be approved or explicitly waived.
- `require_high_risk_only`: representation, plan, code architecture, validation,
  and high-risk artifacts require approval.

## Commands

```bash
python -m veritas_ingest.cli review-checkpoint \
  --phase representation_review \
  --decision approve \
  --policy require_all \
  --artifact-json '{"invariants":["I(T(x))=I(x)"]}'
```

```bash
python -m veritas_ingest.cli review-workflow --policy require_all --decision approve
```

```bash
scripts/e2e/source-mocked-human-workflow.sh
```

## Persistence

Phase 7 writes:

```text
human_checkpoints.jsonl
human_workflow_report.json
human_checkpoints.ttl
human_checkpoint_search_records.jsonl
events.jsonl
```

The API also exposes human checkpoint state through `/status/:run_id` and stores
checkpoint summaries in final reports.
