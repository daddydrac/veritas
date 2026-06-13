# Phase 9 — Evidence-Grounded Planning

Phase 9 makes planning a real application gate: Veritas must build a `planning_context.json` from approved evidence before the planner model is called.

The planner is not allowed to plan from generic model memory. The planning context is built from real run artifacts:

- `evidence_registry.json`
- `evidence_manifest.json`
- `formula_manifest.json`
- `citation_manifest.json`
- OpenSearch retrieval results
- Fuseki/SPARQL formula trace results
- ontology planner facts
- SHACL status when available
- representation model when available

## Required behavior

Production-bound planning requires approved evidence and approved citation provenance. If those are missing, Veritas returns `planning_context.no_approved_evidence` before the planner model is called.

The planner receives:

```text
approved_evidence_ids
approved_citation_ids
eligible_formula_ids
allowed_lineage_ids
```

and every planner step must cite only those ids.

## Dev exploratory mode

`VERITAS_ALLOW_EMPTY_EVIDENCE` is honored only when `execution_mode=dev_exploratory`. That path is non-production and the Artifact Decision Engine marks it as `dev_only_unverified`.

## Output artifact

A successful planning gate writes:

```text
planning_context.json
```

This artifact is later merged with the lineage context and included in the final report.

This phase enforces evidence-grounded planning in the application path.
