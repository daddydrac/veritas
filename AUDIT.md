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
