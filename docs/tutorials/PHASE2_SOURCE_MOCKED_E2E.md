# Phase 2 — Source-Mocked Control-Plane E2E Proof

Phase 2 proves the Veritas control-plane cascade without requiring Cargo, Docker, OpenSearch, Fuseki, SHACL, or live vLLM to run inside the development sandbox.

The source-mocked proof is intentionally narrower than live production validation. It verifies that Veritas can move through the same artifact sequence expected from `/run`:

```text
request
→ evidence snapshot
→ formula trace snapshot
→ schema-valid planner output
→ schema-valid math reasoning output
→ schema-valid codegen output
→ generated files
→ simulated validation failure
→ schema-valid repair output
→ simulated passing validation
→ command audit
→ final report
```

## Command

```bash
scripts/e2e/source-mocked-control-plane-e2e.sh
```

The script writes:

```text
data/e2e/source-mocked-run-response.json
data/e2e/source-mocked-control-plane/summary.json
data/e2e/source-mocked-control-plane/final_report.json
data/e2e/source-mocked-control-plane/events.jsonl
data/e2e/source-mocked-control-plane/command_audit.jsonl
data/e2e/source-mocked-control-plane/planner.json
data/e2e/source-mocked-control-plane/math_reasoning.json
data/e2e/source-mocked-control-plane/codegen_attempt_1.json
data/e2e/source-mocked-control-plane/repair.json
data/e2e/source-mocked-control-plane/workspace/
```

## What it proves

It proves source-level contract coherence:

- planner output satisfies `schemas/planner.schema.json`,
- math reasoning satisfies `schemas/math_reasoning.schema.json`,
- code generation satisfies `schemas/codegen.schema.json`,
- repair satisfies `schemas/repair.schema.json`,
- the final report satisfies `schemas/run_report.schema.json`,
- failures are recorded before repair,
- final artifact status changes only after simulated passing validation.

## What it does not prove

It does not prove live host execution:

- Rust compilation is still `host_validation_pending`,
- Docker Compose execution is still `host_validation_pending`,
- live vLLM/GPU model loading is still `host_validation_pending`,
- live OpenSearch/Fuseki/SHACL validation is still `host_validation_pending`.

Those checks remain part of live host acceptance and are deliberately separated from source/mocked acceptance.
