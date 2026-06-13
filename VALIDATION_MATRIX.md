# Veritas Production Validation Matrix


## Phase 0 — Packaging, Validation, and Scoring Cleanup

This phase separates source/mocked acceptance from live host acceptance. Cargo, Docker Compose E2E, and live vLLM/GPU smoke validation remain host-validation dimensions and are not treated as failed in source/mocked acceptance when those tools are unavailable.

| Capability | Source/mocked acceptance | Live host acceptance | Notes |
|---|---:|---:|---|
| Shell scripts preserve executable bits | implemented | n/a | `scripts/check-packaging.sh` fails if any `scripts/**/*.sh` file is not executable. |
| CI workflow definitions | implemented | pending remote CI | Python, Rust, and Docker fake-vLLM E2E workflow files are present under `.github/workflows/`. |
| Validation script robustness | implemented | n/a | `scripts/validate-spec.py` reports missing files as structured failed checks instead of crashing during Pass 5 workflow inspection. |
| Acceptance profiles | implemented | pending host run | `scripts/production-acceptance.sh --profile source-mocked` performs mocked/source acceptance; production GPU profiles require live vLLM smoke. |
| Host validation summary | implemented | pending host run | `scripts/validate-host.sh` writes `data/e2e/host-validation-summary.json` with skipped/passed step details. |
| Release packaging | implemented | n/a | `scripts/package-release.sh` runs packaging checks and creates ZIPs while preserving executable bits. |


| Capability | Source implemented | Python tests | Rust tests | Docker/live validated | Notes |
|---|---:|---:|---:|---:|---|
| CLI setup writes `.veritas` config | yes | partial | not run | not run | Cargo unavailable here. |
| vLLM planner/code/math config | yes | n/a | not run | not run | Docker/GPU unavailable here. |
| OpenSearch FAISS/HNSW mapping | yes | yes | not run | not run | Mapping tested with fake client. |
| PDF chunking at 25 words + formula preservation | yes | yes | n/a | not run | Docling live validation still host-dependent. |
| APA citation metadata | yes | yes | n/a | not run | Human confirmation still CLI future work. |
| Fuseki RDF output | yes | yes | not run | not run | Turtle parse tested; live Fuseki unavailable. |
| SHACL rule pack/service | yes | rule presence | not run | not run | Live pySHACL service requires Docker. |
| `/plan` model-backed planning | yes | n/a | not run | not run | Requires cargo + vLLM host validation. |
| `/run` bounded code/test loop | yes | n/a | not run | not run | Requires cargo + vLLM host validation. |
| Legacy codegen quarantine | yes | yes | n/a | n/a | No false production status in Python path. |

| Role-specific structured-output routing | yes | source grep | not run | not run | Planner/codegen/math schemas are selected by role; live vLLM structured decoding still host-validated. |
| Pass 1 provider abstraction | yes | source grep | not run | not run | `providers.rs` defines `ModelProvider`, local vLLM, remote OpenAI-compatible fallback, provider router, and failure taxonomy; live Rust/vLLM validation still requires host tools. |
| Fake vLLM E2E profile | yes | fake server compiles | not run | not run | `docker-compose.e2e.yml` added; Docker unavailable here. |
| Math-to-code endpoint | yes | source grep | not run | not run | `/math-to-code` wraps run loop with formula-aware goal; deeper dedicated workflow remains next iteration. |
| Run status/resume/cancel | yes | source grep | not run | not run | Durable run state, locking, cancellation, and artifact-aware resume are source-implemented; live Rust/Docker validation still requires host tools. |
| Sandbox command runner | yes | source grep | not run | not run | Docker sandbox path added; requires host Docker validation. |
| Formula image metadata | yes | yes | n/a | not run | Optional PyMuPDF rasterization with explicit fallback statuses. |


| Pass 2 execution safety | yes | source grep | not run | not run | Durable run workspaces, atomic `run.lock`, persisted request/state/events/artifacts, resumable `/run/:run_id/resume`, cancellation marker, and command audit log added; live Rust/Docker validation still requires host tools. |

| Pass 3 retrieval and ontology hardening | yes | yes | not run | not run | Rust API now owns versioned OpenSearch mapping/migration with aliases, exposes graph upload/list/describe/facts endpoints, groups ingestion RDF by document named graph, and writes run/validation facts back to Fuseki; live OpenSearch/Fuseki validation still requires Docker. |
| OpenSearch versioned aliases | yes | source grep | not run | not run | `/opensearch/migrate` creates a versioned FAISS/HNSW index and read/write aliases; live alias mutation must be host-validated. |
| Fuseki named graph discipline | yes | Turtle parse | not run | not run | Ontology graph, document graph, run graph, and validation graph are explicit; ingestion uploads document ABox facts per document graph. |
| Planner SPARQL fact summary | yes | source grep | not run | not run | API query pack summarizes validation gaps, risks, formulas, invariants, plans, source artifacts, builds, loops, assumptions, deployment observability, and transfer tests. |

## Pass 4 — Mathematical research workflow

| Capability | Source-level status | Sandbox validation | Host validation |
|---|---:|---:|---:|
| Docling visual formula candidate extraction | implemented | Python tests passed | pending live PDF corpus |
| Formula image metadata and rasterization hook | implemented | Python tests passed | pending PyMuPDF + real PDFs |
| Pluggable LaTeX OCR | implemented | Python tests passed | pending configured OCR provider |
| Human formula review | implemented | Python tests passed | pending CLI/Docker host run |
| Representation-first math reasoning schema | implemented | schema parse + validator passed | pending live vLLM |
| Math-to-code human checkpoint | implemented | source validator passed | pending Rust/API host run |
| SHACL math rule pack | implemented | RDF/SHACL files present | pending live SHACL service |

## Pass 5 — Deployment and production proof

| Capability | Source-level status | Sandbox validation | Host validation |
|---|---:|---:|---:|
| Fake-vLLM planner/code/math E2E profile | implemented | file/script tests | pending Docker host |
| Fake embedding service for CI E2E | implemented | Python compile/tests | pending Docker host |
| Sample PDF fixture for ingestion E2E | implemented | file tests | pending Docker host |
| Full fake-vLLM E2E script | implemented | source tests | pending Docker host |
| OpenSearch migration proof in E2E | implemented | source tests | pending Docker host |
| Fuseki ontology upload proof in E2E | implemented | source tests | pending Docker host |
| `/plan` and `/run` E2E assertions | implemented | source tests | pending Docker host |
| GPU layout validation | implemented | source tests | pending NVIDIA host |
| Live vLLM smoke validation | implemented | source tests | pending GPU host |
| Host validation script | implemented | source tests | pending host run |
| Production acceptance script | implemented | source tests | pending host run |
| GitHub Actions CI definitions | implemented | source tests | pending remote CI |
## Phase 0 — Scoped acceptance and host-validation boundary

This repository now separates **source/mocked acceptance** from **live host acceptance** so the validation matrix does not overstate unexecuted host-only checks.

| Check | Current scoped status | Meaning |
|---|---|---|
| `cargo.check` | `host_validation_pending` | Rust compile/test remains a host validation item and is intentionally excluded from this sandbox/source acceptance score. |
| `docker.compose.config` | `host_validation_pending` | Docker Compose validation remains a host validation item and is intentionally excluded from this sandbox/source acceptance score. |
| `live_vllm_smoke` | `host_validation_pending` | Live vLLM/GPU validation remains a host validation item and is intentionally excluded from this sandbox/source acceptance score. |
| source/mocked acceptance | active | Python tests, schema/source validators, fake-vLLM harness files, packaging checks, and documentation checks are in scope. |
| live host acceptance | pending host run | Cargo, Docker Compose, fake-vLLM Docker execution, live OpenSearch/Fuseki/SHACL, and live vLLM are validated only on a proper host. |

Phase 0 target: all non-host validation checks must pass locally, every shell script must be executable, CI workflow files must be present, and `scripts/validate-spec.py` must report structured failures rather than crashing.

## Phase 1 — Provider Abstraction and Structured-Output Enforcement

| Capability | Source/mocked acceptance | Live host acceptance | Notes |
|---|---:|---:|---|
| Full JSON Schema validation | implemented | n/a | `apps/api/src/schemas.rs` validates outputs with the Rust `jsonschema` crate before domain-specific checks run. |
| Planner schema contract | implemented | pending live vLLM | Planner output must pass `schemas/planner.schema.json`, including allowed tool enums and non-empty steps. |
| Codegen schema contract | implemented | pending live vLLM | Codegen output must include package name, language, file path/content pairs, and commands. |
| Math reasoning schema contract | implemented | pending live vLLM | Math output must preserve the representation-first fields from the Veritas math workflow. |
| Provider health | implemented | pending live vLLM | Local vLLM health checks call `/v1/models` and verify the configured served model. |
| Provider retry/backoff | implemented | pending live vLLM | Retry policy is configurable through provider retry environment variables. |
| Provider circuit breaker | implemented | pending live vLLM | Repeated retryable failures open per-provider/per-role circuits. |
| Remote fallback controls | implemented | pending configured remote endpoint | Remote fallback is explicit and supports per-role model overrides. |
| Fake-vLLM structured-output failures | implemented | n/a | Fake vLLM can emit invalid JSON or unknown planner tools for schema tests. |

## Phase 2 — Source-Mocked Control-Plane E2E Proof

This phase keeps Cargo, Docker Compose execution, and live vLLM/GPU validation outside the scoped sandbox acceptance, but adds a runnable source/mocked E2E proof that exercises the Veritas control-plane contracts with schema-valid fake planner, math, codegen, repair, command-audit, and final-report artifacts.

| Capability | Source/mocked acceptance | Live host acceptance | Notes |
|---|---:|---:|---|
| Source-mocked control-plane E2E | implemented | n/a | `scripts/e2e/source-mocked-control-plane-e2e.sh` creates a run workspace, writes request/state/events, validates planner/math/codegen/repair/run-report schemas, simulates a failed validation, repairs it, and emits a validated final report. |
| Final report schema proof | implemented | n/a | The mocked run validates `final_report.json` against `schemas/run_report.schema.json` and reuses `scripts/e2e/assert-e2e-result.py`. |
| Provider/structured-output mock proof | implemented | pending live vLLM | The source E2E uses schema-valid fake outputs and keeps live vLLM validation as a separate host acceptance concern. |
| CI source E2E hook | implemented | pending remote CI | `.github/workflows/python.yml` now runs the source-mocked control-plane E2E after Python tests. |
| Host validator source E2E step | implemented | pending host run | `scripts/validate-host.sh` runs the source-mocked E2E before optional Cargo/Docker/live vLLM gates. |

Phase 2 increases source/mocked confidence without claiming that Rust compilation, Docker Compose execution, or live vLLM model loading has occurred in this sandbox.

## Phase 3 — Execution Safety Hardening

| Capability | Source/mocked acceptance | Live host acceptance | Notes |
|---|---:|---:|---|
| Production sandbox default | implemented | pending Docker host | `VERITAS_PROFILE=*prod` defaults to sandbox unless explicitly overridden. |
| Local runner guardrail | implemented | n/a | Production profiles block local shell unless `VERITAS_ALLOW_LOCAL_COMMAND_RUNNER=true`. |
| Command allowlist hardening | implemented | n/a | Dangerous shell/system tokens such as `curl`, `sudo`, `docker`, `;`, `&&`, pipes, redirects, and inline code are rejected. |
| Docker sandbox resource flags | implemented | pending Docker host | Sandbox command construction includes no network, CPU/memory/pids limits, read-only root, tmpfs `/tmp`, cap drop, and no-new-privileges. |
| Canonical path safety | implemented | source/mocked tested | Generated paths are checked for relative-only components, symlink parents, symlink targets, and post-write containment. |
| Run index persistence | implemented | source/mocked tested | Run state writes `events.jsonl`, `state.json`, and parent `run_index.jsonl`. |
| Resume/cancel safety | implemented | source/mocked tested | Source/mocked proof covers tools pending, validation pending, stale lock, duplicate lock, and cancellation blocking. |
| Command audit visibility | implemented | source/mocked tested | `/status/:run_id` includes command audit tail and lock metadata. |

## Phase 4 — Retrieval and Ontology Source-Mocked Validation

| Capability | Status | Evidence |
|---|---|---|
| OpenSearch mapping contract | source/mocked passed | `services/ingestion/veritas_ingest/retrieval_ontology_contracts.py`, `scripts/e2e/source-mocked-retrieval-ontology.sh` |
| Vector dimension mismatch rejection | source/mocked passed | `assert_vector_dimension` test and Phase 4 E2E summary |
| Versioned index/read-write alias migration | source/mocked passed | `MockOpenSearchTransport` migration simulation |
| Retrieval fallback | source/mocked passed | read alias failure to write alias success simulation |
| Fuseki named graph discipline | source/mocked passed | ontology/document/run/validation graph URI tests |
| No PDF binary in RDF upload | source/mocked passed | `graph_store_request` rejects PDF payload markers |
| Run-report RDF facts | source/mocked passed | SourceCodeArtifact, VerificationResult, BuildArtifact TTL generation |
| Planner SPARQL fact summary | source/mocked passed | query-pack fixture summaries for all planner queries |
| Live OpenSearch/Fuseki validation | host_validation_pending | run live service validation on target host |

## Phase 5 — SHACL and Mathematical Governance Source-Mocked Validation

| Capability | Status | Evidence |
|---|---|---|
| Combined core + math SHACL pack | source/mocked passed | `load_combined_shape_pack`, `shape_pack_contract` |
| SymbolicShadow extraction/readiness obligations | source/mocked passed | missing evidence/source/OCR/confidence/review fixtures fail |
| MathematicalDiscoveryArtifact readiness | source/mocked passed | missing representation/invariant/validation fixtures fail |
| BuildArtifact validation gate | source/mocked passed | validated build without validation fixture fails |
| SHACL findings RDF output | source/mocked passed | `shacl_findings_to_turtle` parses as Turtle |
| Automatic SHACL gate source update | implemented | `default_shacl_shapes`, `collect_shacl_data_ttl`, `shacl_findings_to_turtle` |
| Live SHACL service execution | host_validation_pending | requires Docker/pySHACL host |

## Phase 6 — Formula OCR and Formula Review Source-Mocked Validation

| Capability | Status | Evidence |
|---|---|---|
| Command LaTeX OCR provider contract | source/mocked passed | `command_ocr_contract`, `scripts/e2e/source-mocked-formula-ocr-review.sh` |
| HTTP LaTeX OCR provider contract | source/mocked passed | `http_ocr_contract`, fake HTTP OCR server |
| Formula image metadata contract | source/mocked passed | mock renderer writes deterministic image and metadata |
| OCR fallback states | source/mocked passed | `none`, `heuristic`, `command`, and `http` modes return auditable statuses |
| Formula review persistence | source/mocked passed | approve/edit/reject/skip/auto_approve decisions update chunk JSONL, codegen eligibility, RDF facts |
| APA citation review persistence | source/mocked passed | approve/edit/reject/incomplete decisions update metadata and RDF facts |
| Chunking edge cases | source/mocked passed | abbreviations, no punctuation, multiple formulas, and semicolon boundaries covered |
| OpenSearch mapping for OCR/review metadata | source/mocked passed | formula/citation review fields are mapped as keyword/boolean/float as appropriate |
| Live formula OCR quality on real arXiv corpus | host_validation_pending | run representative corpus validation on target environment |

## Phase 7 — Human Review UX

| Capability | Source/mocked status | Live-host status |
|---|---:|---:|
| Citation/formula/representation/plan/code/validation checkpoints | Implemented and Python-tested | Host validation pending |
| Human checkpoint policy gate | Implemented and Python-tested | Host validation pending |
| Rejection/missing approval blocking | Implemented and Python-tested | Host validation pending |
| Waiver with explicit reason | Implemented and Python-tested | Host validation pending |
| RDF/Turtle `HumanCheckpoint` facts | Implemented and Python-tested | Host validation pending |
| Search-ready checkpoint records | Implemented and Python-tested | Host validation pending |
| `/status/:run_id` checkpoint visibility | Source-level implemented | Rust host validation pending |

<!-- PHASE8_SCORECARD:START -->
## Phase 8 — Documentation and Metric Automation

- Source/mocked average score: **94.06%**.
- All non-skipped source/mocked features are A/B: **True**.
- Live host dimensions are explicitly marked `host_validation_pending`: Rust/Cargo, Docker E2E, and live vLLM/GPU validation.
- Generated artifacts: `data/scorecard/feature-scorecard.json` and `FEATURE_SCORECARD.md`.

See `FEATURE_SCORECARD.md` for the generated feature table.
<!-- PHASE8_SCORECARD:END -->


## Phase 1 Real Journey Orchestrator

| Capability | Status | Evidence |
|---|---|---|
| `/journey/run` API route | Implemented | `apps/api/src/main.rs`, `apps/api/src/journey.rs` |
| Journey status/review/resume/report API routes | Implemented | `apps/api/src/main.rs`, `apps/api/src/journey.rs` |
| CLI `veritas journey ...` commands | Implemented | `apps/cli/src/main.rs` |
| Journey lifecycle artifacts | Implemented | `journey_request.json`, `source_manifest.json`, `journey_state.json`, `journey_lifecycle.jsonl`, `journey_report.json` |
| Real run-core delegation | Implemented | `execute_autonomous_run_core` is called from journey orchestrator |
| Real local ingestion before planning | Implemented source-level | Local backend writes real PDF-derived manifests; Journey blocks before planning when embeddings are unavailable |


## Phase 2 — Real Local Ingestion Backend

| Capability | Status | Evidence |
|---|---|---|
| Local PDF parsing without service DNS | implemented and Python-tested | `services/ingestion/veritas_ingest/cli.py`, `local_backend.py`, `test_phase2_real_local_ingestion_backend.py` |
| Evidence manifest generation | implemented and Python-tested | `evidence_manifest.json` created by local backend |
| Formula/citation manifest generation | implemented and Python-tested | `formula_manifest.json`, `citation_manifest.json` |
| User-visible review queue | implemented and Python-tested | `review_queue.json` |
| RDF/Turtle local graph output | implemented and Python-tested | `evidence.ttl` |
| Local lexical index | implemented and Python-tested | `local_lexical_index.jsonl` |
| Local vector index with no fake embeddings | implemented and Python-tested | `local_vector_index.jsonl` remains empty and planning blocks when no real embedding provider is configured |
| Journey blocks before planning when retrieval is unavailable | source-level implemented | `apps/api/src/journey.rs` returns `blocked_by_retrieval_unavailable` before `execute_autonomous_run_core` |
| Live Rust/API journey execution | host_validation_pending | Requires Cargo/API host execution |

| PHASE3_EVIDENCE_ELIGIBILITY_REGISTRY | Source/Python | Implemented | `evidence_registry.json`, `evidence_eligibility.json`, formula/citation review refresh, `/math-to-code` registry gate, journey planning gate |


## Evidence Eligibility Registry

Phase 3 adds the real Evidence Eligibility Registry. Local ingestion writes `evidence_registry.json` and `evidence_eligibility.json`; formula/citation review decisions are normalized into authoritative planning/codegen gates. A rejected or pending formula blocks `/math-to-code`; an unreviewed citation blocks production-bound evidence-backed planning. Request-level approval booleans are not authoritative.


## Phase 4 — Pre-Execution Gate Engine

| Capability | Status | Evidence |
|---|---|---|
| Gate engine before codegen | Implemented | `apps/api/src/main.rs` invokes `gates::run_pre_codegen_gates` before code generation. |
| Evidence gate | Implemented | `apps/api/src/gates/evidence.rs` reads `evidence_registry.json` and blocks if production evidence is not eligible. |
| Human pre-codegen gates | Implemented | `apps/api/src/gates/human.rs` requires `plan_review` and `code_architecture_review` unless policy/config disables them. |
| Representation gate | Implemented | `apps/api/src/gates/representation.rs` blocks math-heavy runs without `representation_model.json`. |
| Math-tool gate | Implemented as enforcement boundary | `apps/api/src/gates/math_tools.rs` blocks math-heavy runs until real math validation artifacts exist. |
| SHACL pre-codegen gate | Implemented | `apps/api/src/gates/shacl.rs` blocks enforced SHACL failures before codegen. |
| Blocked final report | Implemented | `apps/api/src/gates/mod.rs` writes `pre_codegen_blocked_report.json` and `final_report.json` with no files or commands. |

## Phase 5 — Tool-Verified Math Engine

Veritas now includes a real Tool-Verified Math Engine. Math-heavy runs no longer have to rely only on LLM reasoning before code generation. The application can call the `math-tools` service, persist `math_tool_calls.jsonl`, `math_tool_results.jsonl`, and `math_validation_report.json`, and the pre-codegen Gate Engine blocks when the report contains blocking findings or counterexamples.

The math-tools service exposes real executable tools: `parse_latex`, `normalize_expression`, `symbolic_simplify`, `symbolic_differentiate`, `symbolic_equivalence`, `numeric_validate`, `counterexample_search`, `dimension_check`, and `generate_property_tests`. The service uses SymPy, NumPy, SciPy/mpmath-compatible numeric evaluation, and generated property-test code. No model output is treated as mathematical truth unless tool results, governance gates, and validation artifacts support it.


| Phase 6 SHACL artifact governance | Implemented | `VERITAS_GOVERNANCE_MODE`, artifact bundle TTL, pre-codegen SHACL, final SHACL, and blocked-by-governance behavior added. |


## Phase 7 — Artifact Decision Engine

Phase 7 adds a canonical Artifact Decision Engine in `apps/api/src/artifact_decision.rs`. Final artifact status is no longer granted directly by the code-generation loop. The engine reads real run artifacts, gate decisions, validation results, human checkpoint state, SHACL results, and host-validation evidence before producing `artifact_decision.json`.

Important behavior:

- validation success alone does not imply production readiness;
- missing human approval results in `awaiting_human_approval`;
- failed SHACL results in `blocked_by_governance` when governance is enforced;
- failed validation results in `validation_failed` or `repair_failed`;
- missing host validation results in `local_validated_host_pending`;
- `production_validated` is only possible when host validation evidence exists and passes.

## Phase 8 — lineage schema enforcement

| Capability | Status | Evidence |
|---|---|---|
| Planner step lineage required | Source-level implemented | `schemas/planner.schema.json` |
| Codegen file lineage required | Source-level implemented | `schemas/codegen.schema.json` |
| Final report lineage required | Source-level implemented | `schemas/run_report.schema.json` |
| Runtime lineage validation before file writes | Source-level implemented | `apps/api/src/lineage.rs`, `apps/api/src/main.rs` |
| Unknown lineage IDs rejected | Source-level implemented | `lineage.codegen_invalid`, `lineage.plan_invalid` |
| Host Rust/Docker/live service validation | Pending host | Cargo/Docker unavailable in this sandbox |


| Phase 8 lineage schemas | implemented | source validated | Planner, codegen, and run-report schemas require explicit lineage; `apps/api/src/lineage.rs` validates codegen lineage before file writes. |

## Phase 9 — Evidence-grounded planning

| Capability | Status | Evidence |
|---|---|---|
| Build planning context before planner call | Implemented | `apps/api/src/planning_context.rs`, `apps/api/src/main.rs` |
| Block production-bound planning without approved evidence | Implemented | `planning_context.no_approved_evidence` |
| Validate planner evidence/citation/formula IDs | Implemented | `planning_context::validate_plan_references` |
| Restrict empty-evidence bypass to dev exploratory mode | Implemented | `execution_mode=dev_exploratory` gate |

## Phase 9 — Evidence-grounded planning

| Capability | Status | Evidence |
|---|---|---|
| Planning context artifact | Implemented | `apps/api/src/planning_context.rs`, `schemas/planning_context.schema.json` |
| Production planning blocks without approved evidence | Implemented | `planning_context.no_approved_evidence` gate |
| Planner receives approved IDs only | Implemented | `planner_prompt_contract` and `planning_context::validate_plan_references` |
| Empty-evidence bypass restricted to dev | Implemented | `execution_mode=dev_exploratory` and `dev_only_unverified` |
| Runtime validation | Host pending | Cargo/Docker/live services still require target host validation |
