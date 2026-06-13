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


## Phase 0 completion update — packaging, validation, and scoring cleanup

Phase 0 separates **source/mocked acceptance** from **live host acceptance**. The project now treats `cargo.check`, `docker.compose.config`, and `live_vllm_smoke` as `host_validation_pending` when Cargo, Docker, or GPU/vLLM runtime are unavailable. Those checks remain required for live host acceptance, but they are not scored as failed source-level checks during mocked/source validation.

Phase 0 also enforces release packaging hygiene. Every shell script under `scripts/` must be executable, and `scripts/check-packaging.sh` fails if any `*.sh` file lacks execute permissions. The GitHub Actions workflow files for Python, Rust, and Docker fake-vLLM E2E are present under `.github/workflows/`.

`scripts/production-acceptance.sh` now supports explicit profiles: `fake-ci`, `single-gpu-prod`, `multi-gpu-prod`, and `remote-model-prod`. The script records whether the run is `mocked_acceptance`, `live_gpu_acceptance`, or `remote_model_acceptance` so the audit trail cannot confuse a source/mocked proof with live GPU/vLLM production proof.

Host-validation markers:

```text
cargo.check = host_validation_pending
docker.compose.config = host_validation_pending
live_vllm_smoke = host_validation_pending
source/mocked acceptance = active
live host acceptance = pending host run
```


## Phase 2 completion update — source-mocked control-plane E2E proof

Phase 2 is source/mocked-complete for the planned E2E proof scope. The project now includes `scripts/e2e/source-mocked-control-plane-e2e.py` and `scripts/e2e/source-mocked-control-plane-e2e.sh`, which run without Cargo, Docker, OpenSearch, Fuseki, SHACL, or live vLLM. The harness validates fake planner, math, codegen, repair, human-checkpoint, and run-report payloads against the same JSON Schema contracts used by the API, writes a run workspace, intentionally fails the first validation attempt, applies a repair payload, reruns validation, records `command_audit.jsonl` and `events.jsonl`, and emits a schema-valid `final_report.json` with `production_candidate_validated` status.

This proves the control-plane artifact cascade at source/mocked level while preserving the live-host boundary: Rust compilation, Docker Compose execution, live OpenSearch/Fuseki/SHACL, and live vLLM/GPU validation remain `host_validation_pending` until executed on a proper host. `scripts/validate-host.sh --profile source-mocked` now runs packaging checks, focused Python phase tests, and the source-mocked control-plane E2E before marking source/mocked acceptance complete.

## Generated audit snapshot

Generated at: 2026-06-11T23:11:34.966790+00:00

```json
{
  "total": 90,
  "failed": 0,
  "unavailable": 2
}
```

## Phase 5 completion update — SHACL and mathematical governance

Phase 5 is source/mocked-complete for the planned SHACL/math-governance scope. The automatic SHACL gate now composes both `packages/ontology/shacl/veritas-core.shacl.ttl` and `packages/ontology/shacl/veritas-math.shacl.ttl`, persists the SHACL data/shapes/report/findings artifacts, and records graph-derived SHACL context status. The source/mocked governance proof verifies that complete math-to-code RDF conforms, while incomplete symbolic shadows, incomplete mathematical discovery artifacts, and production-validated build artifacts without validation are blocked.

Command:

```bash
scripts/e2e/source-mocked-shacl-governance.sh
```

Live SHACL container execution remains `host_validation_pending` and is intentionally not claimed in the source/mocked scope.

## Phase 3 completion update — execution safety hardening

Phase 3 remains source/mocked-complete. Production-like profiles default to sandbox execution, generated file paths are confined to the run workspace, command policy rejects dangerous shell/system tokens, run locks and run indexes are persisted, and source/mocked resume/cancel tests cover active locks, stale locks, validation-pending resumes, and cancelled-run blocking.

## Phase 4 completion update — retrieval and ontology hardening

Phase 4 remains source/mocked-complete. OpenSearch mapping, vector dimension rejection, versioned alias migration, retrieval fallback, Fuseki named graph discipline, graph-store upload contracts, no-PDF-binary RDF upload, run-report RDF facts, and SPARQL planner fact summaries are validated by the source/mocked retrieval ontology proof.

## Phase 6 completion update — formula OCR and review contracts

Phase 6 is source/mocked-complete for the planned formula OCR and review scope. The ingestion layer now supports command and HTTP LaTeX OCR providers, deterministic mock formula-image rendering for CI/source validation, richer formula image metadata, citation review decisions, formula review decisions, codegen eligibility status, and OpenSearch/Fuseki persistence of those fields.

Command:

```bash
scripts/e2e/source-mocked-formula-ocr-review.sh
```

This proves the Formula → Image/OCR → Review → OpenSearch/RDF metadata contract with mocked OCR providers. Live visual OCR quality against a representative arXiv corpus remains `host_validation_pending` because it depends on the target OCR provider, Docling/PyMuPDF behavior, and the paper corpus.

## Phase 7 — Human Review Workflow Audit

Status: source/mocked complete; live host validation remains out of scope for
this phase.

Implemented:

- Shared human checkpoint state machine.
- Checkpoint phases for citation, formula, representation, plan, code
  architecture, and validation review.
- Policy gate support for `auto_approve`, `require_all`, and
  `require_high_risk_only`.
- Rejection and pending required checkpoints block workflow progress.
- Explicit `skip` with notes records a waiver.
- Checkpoints persist to `human_checkpoints.jsonl`, `events.jsonl`, RDF/Turtle,
  search records, and human workflow reports.
- API `/human/checkpoint` accepts all Phase 7 checkpoint phases and `/status/:run_id`
  returns checkpoint gate state.
- Source/mocked proof: `scripts/e2e/source-mocked-human-workflow.sh`.

Validation run expected in this environment:

```bash
scripts/e2e/source-mocked-human-workflow.sh
PYTHONPATH=services/ingestion pytest -q tests/ingestion/test_phase7_human_workflow.py
```

<!-- PHASE8_SCORECARD:START -->
## Phase 8 — Documentation and Metric Automation

- Source/mocked average score: **94.06%**.
- All non-skipped source/mocked features are A/B: **True**.
- Live host dimensions are explicitly marked `host_validation_pending`: Rust/Cargo, Docker E2E, and live vLLM/GPU validation.
- Generated artifacts: `data/scorecard/feature-scorecard.json` and `FEATURE_SCORECARD.md`.

See `FEATURE_SCORECARD.md` for the generated feature table.
<!-- PHASE8_SCORECARD:END -->


## Phase 1 Real Journey Orchestrator Audit

Status: source-level implemented.

The real journey orchestrator adds API endpoints and CLI commands for a single end-user workflow. It creates a real run workspace, records source and lifecycle artifacts, accepts human journey reviews, supports status/resume/report, and delegates to the existing autonomous run core rather than source-mocked scripts.

Phase 2 now attaches real local ingestion artifacts before planning/codegen. Later phases must add evidence-registry gates, pre-codegen human gates, default-on SHACL enforcement, tool-verified math, artifact decision logic, and behavior-derived scorecards.


## Phase 2 Real Local Ingestion Backend Audit

Status: implemented at application/source level; live Rust/API execution remains host-validation pending.

The local ingestion backend now parses real PDFs, writes `chunks.jsonl`, `formulas.jsonl`, `citations.jsonl`, `evidence.ttl`, `local_lexical_index.jsonl`, `local_vector_index.jsonl`, `evidence_manifest.json`, `formula_manifest.json`, `citation_manifest.json`, `review_queue.json`, and `ingestion_report.md`. The backend never fabricates embeddings. If no real embedding provider is available, `evidence_manifest.json` sets `planning_status=blocked_retrieval_unavailable`, and the Journey orchestrator blocks before delegating into planning/codegen.

Validation evidence in this environment:

```bash
PYTHONPATH=services/ingestion pytest -q tests/ingestion/test_phase2_real_local_ingestion_backend.py
python3 scripts/validate-spec.py
```

Remaining work: later phases must make formula/citation review decisions globally authoritative and enforce all human/governance gates before codegen.

## Phase 3 audit note: Evidence Eligibility Registry

Implemented the first causal evidence gate. Local ingestion now writes `evidence_registry.json` and `evidence_eligibility.json`. Citation and formula review commands refresh the registry from the updated chunks file. `/math-to-code` now blocks rejected, pending, low-confidence, or missing-citation formulas before model calls. Journey mode reads the registry and blocks planning with `awaiting_evidence_review` when citations/formulas are not eligible.

Host-only validation remains pending for Rust compilation and live services, but the Python ingestion/registry path is executable in this environment.

## Phase 4 Audit — Pre-Execution Gate Engine

Phase 4 addresses the prior critical gap where human checkpoints were checked after code generation and validation. The real application path now invokes `gates::run_pre_codegen_gates` before `build_code_generation_prompt`, `write_generated_files`, and `run_command` execute.

Resolved behavior:

- Missing `plan_review` blocks before codegen.
- Missing `code_architecture_review` blocks before codegen.
- Rejected or ineligible evidence blocks before codegen.
- Math-heavy runs require representation and math-tool readiness artifacts before codegen.
- Enforced SHACL failures block before codegen.
- Blocked runs write final reports with `files_changed=[]` and `commands_run=[]`.

Remaining future work:

- Phase 5 has added the real Tool-Verified Math Engine that produces `math_validation_report.json` and gates math-heavy codegen through tool results.
- Phase 6 will make SHACL governance mode default-on and artifact-bundle based.
- Phase 7 will replace direct final-status mutation with the Artifact Decision Engine.

## Phase 5 — Tool-Verified Math Engine

Veritas now includes a real Tool-Verified Math Engine. Math-heavy runs no longer have to rely only on LLM reasoning before code generation. The application can call the `math-tools` service, persist `math_tool_calls.jsonl`, `math_tool_results.jsonl`, and `math_validation_report.json`, and the pre-codegen Gate Engine blocks when the report contains blocking findings or counterexamples.

The math-tools service exposes real executable tools: `parse_latex`, `normalize_expression`, `symbolic_simplify`, `symbolic_differentiate`, `symbolic_equivalence`, `numeric_validate`, `counterexample_search`, `dimension_check`, and `generate_property_tests`. The service uses SymPy, NumPy, SciPy/mpmath-compatible numeric evaluation, and generated property-test code. No model output is treated as mathematical truth unless tool results, governance gates, and validation artifacts support it.


## Phase 6 audit update

SHACL is now governed by `VERITAS_GOVERNANCE_MODE` and validates an artifact bundle built from real run artifacts. Pre-codegen SHACL blocks generated files and validation commands in enforce mode. Final SHACL runs after validation and can downgrade the artifact lifecycle to `blocked_by_governance`.


## Phase 7 — Artifact Decision Engine

Phase 7 adds a canonical Artifact Decision Engine in `apps/api/src/artifact_decision.rs`. Final artifact status is no longer granted directly by the code-generation loop. The engine reads real run artifacts, gate decisions, validation results, human checkpoint state, SHACL results, and host-validation evidence before producing `artifact_decision.json`.

Important behavior:

- validation success alone does not imply production readiness;
- missing human approval results in `awaiting_human_approval`;
- failed SHACL results in `blocked_by_governance` when governance is enforced;
- failed validation results in `validation_failed` or `repair_failed`;
- missing host validation results in `local_validated_host_pending`;
- `production_validated` is only possible when host validation evidence exists and passes.

## Phase 8 audit update — lineage schemas and runtime enforcement

Status: implemented at source/application level.

The previous audit identified incomplete artifact-to-evidence lineage. Phase 8 strengthens lineage by requiring planner step IDs, evidence IDs, citation IDs, formula IDs, risk IDs, validation gate IDs, and human checkpoint IDs in the planner schema. Codegen output must include per-file and per-command lineage. `apps/api/src/lineage.rs` validates these references before `write_generated_files`, so lineage is now a pre-write gate rather than report decoration.

Host validation remains pending where Cargo/Docker/live services are unavailable in this sandbox.


## Phase 8 — Lineage Schema Enforcement

Resolved gap: final reports and generated files previously carried useful audit information, but lineage was not required by schema or enforced before writes. Phase 8 adds `apps/api/src/lineage.rs`, strengthens planner/codegen/run-report schemas, writes `planning_context.json` and `file_lineage.json`, validates codegen lineage before file writes, and requires report lineage fields.

Remaining host-only validation: Cargo/Rust compilation, Docker Compose E2E, and live vLLM/GPU validation remain unavailable in this sandbox.

## Phase 9 audit note — evidence controls planner execution

Resolved gap: retrieval and evidence metadata are no longer passive inputs for planning. The planner is now called only after `planning_context.json` is built from approved evidence, and planner output must cite IDs from that context. Empty-evidence planning is limited to `execution_mode=dev_exploratory` and cannot claim production readiness.

## Phase 9 audit — Evidence-grounded planning

Phase 9 resolves the gap that planning could still proceed without a mandatory approved-evidence context. The API now uses `apps/api/src/planning_context.rs` to build `planning_context.json` before planner invocation. The context is generated from the Evidence Eligibility Registry, approved citations, eligible formulas, retrieval results, formula trace SPARQL output, ontology facts, SHACL status, and representation artifacts.

Production-bound planning blocks with `planning_context.no_approved_evidence` when approved evidence or approved citation provenance is missing. `VERITAS_ALLOW_EMPTY_EVIDENCE` is restricted to `execution_mode=dev_exploratory`, and the Artifact Decision Engine classifies that path as `dev_only_unverified`.
