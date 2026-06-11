# Pass 5 — Deployment and Production Proof

Pass 5 is the proof layer for Veritas. Earlier passes harden the control plane, execution safety, retrieval/ontology stack, and mathematical workflow. Pass 5 proves those components together.

## Fake-vLLM E2E

Run:

```bash
scripts/e2e/full-fake-vllm-e2e.sh
```

Expected workflow:

1. Generate `.veritas/runtime.env` for fake model serving.
2. Start OpenSearch, Fuseki, SHACL, API, fake vLLM planner/code/math, and fake embedding.
3. Wait for `/ready`.
4. Create the OpenSearch FAISS/HNSW mapping and aliases.
5. Upload the Veritas ontology into Fuseki.
6. Ingest the sample PDF fixture.
7. Verify OpenSearch and Fuseki are queryable.
8. Call `/plan`.
9. Call `/run`.
10. Validate the final report.

## Host validation

Run:

```bash
scripts/validate-host.sh
```

This is the required local production validation command. It runs Python tests, Rust checks, Docker Compose config, GPU layout validation, and fake-vLLM Docker E2E.

## Live vLLM validation

Run on a GPU host:

```bash
VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true scripts/validate-host.sh
```

This proves that planner, code, and math vLLM services expose `/v1/models` and can be reached through the configured ports.

## Acceptance boundary

Do not claim production certification unless the host validation scripts pass on the target hardware. Fake-vLLM E2E proves orchestration. Live vLLM proof validates model serving. Cargo validates Rust code. Docker Compose validates deployability.
