# Veritas Audit

## Current status

This repository now implements the next production-hardening pass toward the Veritas specification. The source-level path includes CLI setup/deployment generation, vLLM role configuration, OpenSearch FAISS/HNSW evidence indexing, Fuseki RDF/SPARQL graph upload, SHACL rule-pack support, formula-preserving chunking, citation extraction, bounded `/run` orchestration, command allowlisting, state/event persistence, and no legacy path that can falsely mark generated scaffolds as production-validated.

## Resolved source-level gaps

1. `/plan` is model-backed and schema-validated through the Rust API path.
2. `/run` creates a workspace, writes state, requests a plan, writes files, runs commands, retries failures, and writes a final report.
3. The legacy Python codegen path no longer emits `production_candidate_validated`; it emits `generated_unvalidated`.
4. OpenSearch is the only vector index; Qdrant/property graph claims remain out of scope.
5. SHACL service and rule pack have been added.
6. The CLI setup flow writes `.veritas/runtime.env` and `.veritas/config.yaml`; `.env.example` is intentionally removed.
7. Docker Compose includes vLLM planner/code/math services with structured-output backend flags and per-role GPU/tensor/pipeline variables.
8. PDF ingestion now produces APA citation metadata, formula-aware chunks, and RDF/OpenSearch records with richer metadata.
9. Formula extraction remains honest: Docling/regex extraction is best-effort and corpus validation is required.

## Validation executed in this sandbox

```bash
python3 -m compileall services/embedding services/ingestion services/shacl
PYTHONPATH=services/ingestion pytest -q tests/ingestion -q
python3 scripts/validate-spec.py
```

Results:

```text
Python compile: passed
Python tests: 14 passed
scripts/validate-spec.py: ok=true, failed=0, unavailable=2
```

## Commands not executed here

The following commands require a host with Rust, Docker, and GPU/model runtime:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
docker compose --env-file .veritas/runtime.env config
docker compose --env-file .veritas/runtime.env up -d
docker compose --env-file .veritas/runtime.env run --rm cli run "Implement the indexed formula as tested Rust code" --language rust
```

## Remaining host-validation boundary

I cannot honestly claim live production E2E until those commands pass on a real Docker/Rust/GPU host. The source files have been updated to meet the intended behavior, but production certification still requires live service startup, model loading, OpenSearch indexing, Fuseki querying, SHACL validation, and `/run` execution on target hardware.

## Additional hardening added after latest audit

The latest source update adds role-specific schema routing, fake-vLLM E2E scaffolding, OpenSearch mapping/migration endpoints, math-to-code endpoint, run status/resume/cancel endpoints, automatic SHACL gating before codegen, optional Docker sandbox command runner, formula image extraction metadata, and expanded ontology/RDF fields for formula image and human validation state.

New commands validated in this sandbox:

```bash
python3 -m compileall services/embedding services/ingestion services/shacl tests/fakes
PYTHONPATH=services/ingestion pytest -q tests/ingestion -q
python3 scripts/validate-spec.py
```

Observed result:

```text
Python compile: passed
Python tests: 19 passed
scripts/validate-spec.py: ok=true, failed=0, unavailable=2
```

Important remaining boundary: Rust and Docker execution still could not be run in this environment because Cargo and Docker are unavailable. The new fake-vLLM Docker profile and sandbox runner are source-level additions that must be validated on a Docker host before the project can claim complete production E2E compliance.

## Pass 1 completion update — control-plane safety

Pass 1 is now source-complete for the planned control-plane safety scope. The API no longer owns provider calls as loose helper-only routing. It now includes:

- `apps/api/src/providers.rs` with a real `ModelProvider` trait.
- `LocalVllmProvider` for the default vLLM OpenAI-compatible path.
- `RemoteOpenAICompatibleProvider` for explicitly configured remote fallback.
- `ProviderRouter` for local-first routing, retryable-error fallback, route annotation, and provider failure classification.
- `ProviderFailureCategory` for transport, timeout, unavailable model, GPU OOM, context length, rate limit, auth, schema, JSON, and upstream failures.
- `apps/api/src/schemas.rs` with role-specific JSON schemas loaded from `schemas/*.schema.json` through `include_str!`.
- `ApiFailure::from_provider_error` so provider failures become meaningful user-facing API errors with remediation.
- `scripts/validate-spec.py` now checks for the Pass 1 provider abstraction directly.

This resolves the previous source-level gap that the API used direct helper-style model calls rather than a true provider abstraction. It does not yet prove live vLLM behavior; that remains blocked by unavailable Cargo/Docker/GPU execution in this sandbox.


## Pass 2 completion update — execution safety

Pass 2 is now source-complete for the planned execution-safety scope. Veritas now persists every run as a durable workspace with `request.json`, `state.json`, `events.jsonl`, `plan_envelope.json`, `tool_outputs.json`, `automatic_shacl_report.json`, generated code package snapshots, command audit logs, validation result snapshots, retry history, and `final_report.json`. The API now uses an atomic `run.lock` file with stale-lock protection to prevent two workers from advancing the same run concurrently. `/status/:run_id` reads persisted state from disk instead of relying only on in-memory recent runs. `/run/:run_id/resume` now reloads `request.json`, reuses persisted plan/tool/SHACL artifacts when available, and continues the bounded code-generation/validation loop instead of returning a placeholder. `/run/:run_id/cancel` writes a cancellation marker and records a cancel event that the run loop checks between generation and validation steps. Command execution now writes `command_audit.jsonl` in addition to structured validation results.

This completes the source-level Pass 2 target: generated code execution has a safer audit path, run state is durable, cancellation is observable, and resume semantics are implemented around persisted artifacts and run locking. Live validation still requires Cargo and Docker on a host machine.
## Pass 5 completion update — deployment and production proof

Pass 5 is source-complete for the deployment/proof harness. The repo now includes a Docker fake-vLLM E2E profile, fake embedding service, sample math PDF fixture, service readiness waits, OpenSearch migration proof, ontology upload proof, PDF ingestion proof, `/plan` proof, `/run` proof, final-report assertions, GPU layout validation, live vLLM smoke scripts, CI workflows, and strict host-validation/production-acceptance scripts.

Important honesty boundary: this sandbox still cannot execute Cargo, Docker Compose, GPU runtime, live vLLM model loading, live OpenSearch, live Fuseki, or live SHACL. Therefore Pass 5 should be described as source-complete with a host-executable proof harness. Production certification occurs only after `scripts/validate-host.sh` and, when required, `VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true scripts/validate-host.sh` pass on the target machine.

New Pass 5 files and touched areas include:

```text
scripts/e2e/write-fake-runtime-env.sh
scripts/e2e/wait-ready.sh
scripts/e2e/validate-services.sh
scripts/e2e/upload-ontology.sh
scripts/e2e/ingest-fixture.sh
scripts/e2e/plan-fixture.sh
scripts/e2e/run-fixture.sh
scripts/e2e/assert-e2e-result.py
scripts/e2e/full-fake-vllm-e2e.sh
scripts/e2e/gpu-validation.sh
scripts/e2e/live-vllm-smoke.sh
scripts/validate-host.sh
scripts/production-acceptance.sh
docker-compose.e2e.yml
tests/fakes/fake_embedding_server.py
tests/fakes/Dockerfile.embedding
tests/fixtures/sample_math_paper.pdf
data/fixtures/sample_math_paper.pdf
.github/workflows/python.yml
.github/workflows/rust.yml
.github/workflows/docker-e2e.yml
```

## Generated audit snapshot

Generated at: 2026-06-11T18:06:44.702346+00:00

```json
{
  "total": 67,
  "failed": 0,
  "unavailable": 2
}
```
