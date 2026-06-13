# Veritas Feature Scorecard

Generated at: `2026-06-13T18:44:51.803380+00:00`

Scope: **source_mocked**

Source/mocked average score: **94.06%**

Source/mocked all A/B: **True**

Live host dimensions remain `host_validation_pending`: Rust/Cargo validation, Docker E2E validation, and live vLLM/GPU validation.

| Feature | Grade | Score | Evidence | Notes |
|---|---:|---:|---|---|
| Architecture alignment | A | 96% | `README.md, FEATURES.md, docker-compose.yml` | Architecture matches ontology + retrieval + vLLM + validation-loop target. |
| Source-level feature coverage | A- | 95% | `apps/api/src/main.rs, services/ingestion/veritas_ingest, packages/ontology` | Passes 0-7 source features are implemented and validated through source/mocked harnesses. |
| Python/source-mocked test coverage | A | 95% | `tests/ingestion, scripts/e2e/source-mocked-*.sh` | Python tests and source/mocked E2E scripts are part of source/mocked acceptance. |
| Provider abstraction | A- | 92% | `apps/api/src/providers.rs, apps/api/src/schemas.rs` | Provider trait, local vLLM, remote fallback, circuit breaker, retry/backoff, and route history are source-level implemented. |
| Remote fallback | A- | 92% | `apps/api/src/providers.rs, README.md` | Remote fallback is explicit, role-aware, audited, and not silent. |
| Structured outputs | A | 95% | `schemas/*.schema.json, apps/api/src/schemas.rs, docs/STRUCTURED_OUTPUTS.md` | Role-specific schema contracts govern planner, codegen, math, repair, human checkpoint, and run report output. |
| Planner/codegen/math schemas | A | 95% | `schemas/planner.schema.json, schemas/codegen.schema.json, schemas/math_reasoning.schema.json` | Schema validation and fake structured-output tests cover accepted and rejected model outputs. |
| Run state / locking / resume | A- | 93% | `apps/api/src/main.rs, scripts/e2e/source-mocked-execution-safety.sh` | Run state, lock metadata, cancellation, status, command audit, and source/mocked resume semantics are implemented. |
| Sandbox and path safety | A- | 93% | `apps/api/src/main.rs, docker/sandbox/rust.Dockerfile` | Production profiles default toward sandbox behavior and path safety rejects traversal/symlink escape patterns. |
| OpenSearch mapping/migration source proof | A | 95% | `schemas/opensearch/evidence_document.schema.json, services/ingestion/veritas_ingest/retrieval_ontology_contracts.py` | Mapping, FAISS/HNSW fields, aliases, dimension mismatch, and fallback query behavior are source/mocked tested. |
| Fuseki named graph source proof | A | 95% | `packages/ontology/queries, services/ingestion/veritas_ingest/retrieval_ontology_contracts.py` | Ontology/document/run/validation graph discipline and graph-store upload contracts are source/mocked tested. |
| SPARQL fact summary | A | 95% | `packages/ontology/queries, apps/api/src/main.rs` | Planner grounding summarizes the full SPARQL query pack into typed facts. |
| SHACL core/math/gate | A | 95% | `packages/ontology/shacl/veritas-core.shacl.ttl, packages/ontology/shacl/veritas-math.shacl.ttl` | Core and math SHACL rules and source/mocked governance proof are implemented. |
| Formula image/OCR contract | B+ | 90% | `services/ingestion/veritas_ingest/formula_images.py, services/ingestion/veritas_ingest/latex_ocr.py` | Command and HTTP OCR providers are contract-tested with fallback states; live OCR quality remains corpus-dependent. |
| Human review UX | A- | 93% | `services/ingestion/veritas_ingest/human_workflow.py, schemas/human_checkpoint.schema.json` | Citation, formula, representation, plan, code architecture, and validation checkpoints are source/mocked implemented. |
| Documentation and scorecard automation | A | 96% | `FEATURES.md, QUICKSTART.md, VALIDATION_MATRIX.md, AUDIT.md, scripts/generate-feature-scorecard.py` | Scorecard generation separates source/mocked acceptance from live host acceptance. |
| Rust validation | host_validation_pending | host_validation_pending | `.github/workflows/rust.yml` | Skipped in this scoped pass; run cargo fmt/check/test/clippy on a Rust host. |
| Docker E2E validation | host_validation_pending | host_validation_pending | `docker-compose.e2e.yml, scripts/e2e/full-fake-vllm-e2e.sh` | Skipped in this scoped pass; run Docker fake-vLLM E2E on a Docker host. |
| Live vLLM validation | host_validation_pending | host_validation_pending | `scripts/e2e/live-vllm-smoke.sh` | Skipped in this scoped pass; run live vLLM smoke on target GPU/model host. |
