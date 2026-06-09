# Tutorial 003 — Math to Production Code Plan

## Goal
Convert a research/math goal into a validated code-generation plan.

## Steps

```bash
curl -s http://localhost:8080/plan \
  -H 'content-type: application/json' \
  -d '{"goal":"Turn the formula from the retrieved paper into tested Rust and CUDA code"}' | jq
```

## Next implementation

Wire the returned plan envelope to a model router that uses:

- retrieved evidence from OpenSearch
- symbolic formulas from Fuseki
- ontology classes from Veritas OWL
- validation requirements from SHACL/rules
- language-specific code generators
