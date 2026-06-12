# Phase 3 — Evidence Eligibility Registry

The Evidence Eligibility Registry makes formula and citation review decisions causally control downstream execution.

## Behavior

- Local ingestion writes `evidence_registry.json` and `evidence_eligibility.json`.
- Citation review changes whether evidence can support production-bound planning.
- Formula review changes whether a formula can support production-bound math-to-code.
- `/math-to-code` checks the registry before calling any model.
- Journey mode blocks planning when evidence is not eligible.

## Required statuses

Formula codegen eligibility is one of:

```text
eligible
rejected
pending_review
waived_for_exploration
not_eligible_low_confidence
not_eligible_missing_citation
not_eligible_missing_latex
```

Citation planning eligibility requires `citation_usable_for_audit=true`.
