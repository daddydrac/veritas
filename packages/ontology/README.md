# Veritas Ontology

OWL-DL application ontology for math-heavy evidence-backed research and development software engineering.

Use it to connect:

- mathematical symbolic shadows and formulas
- representation maps, invariants, constraints, and proof status
- engineering plans, tasks, risks, mitigations, and validation checks
- source code, build artifacts, runtime specs, and observability signals

## Validate

```bash
docker compose run --rm reasoner consistency /workspace/ontology/veritas.owl
```

For stricter CI, use ROBOT + HermiT externally:

```bash
robot validate-profile --profile DL --input packages/ontology/veritas.owl
robot reason --reasoner HermiT --input packages/ontology/veritas.owl --output data/ontology/inferred.owl
```
