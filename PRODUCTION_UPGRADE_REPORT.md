# Veritas Production Upgrade Report

This update implements the next production-hardening pass toward the Veritas business use case: a non-coder can configure deployment through the CLI, ingest research papers, index text and formula evidence, load ontology facts into Fuseki, use vLLM-served agents through structured JSON contracts, run bounded planning/code/validation loops, and receive auditable artifacts.

## Implemented in this pass

- Removed the `.env.example` dependency. `veritas init` now generates `.veritas/runtime.env`, `.veritas/config.yaml`, and `.veritas/docker-compose.override.yaml`.
- Expanded the CLI setup/deployment wizard to ask about model roles, Hugging Face token, GPU count, per-role GPU IDs, vLLM tensor/pipeline parallelism, GPU memory utilization, OpenSearch, Fuseki, SHACL, document upload, ontology upload, chunking, formula extraction, human-in-loop policy, output path, and FP-style code design preferences.
- Updated Docker Compose to include a SHACL validation service and per-role vLLM GPU/device/tensor/pipeline configuration variables.
- Added vLLM structured-output backend flags to the planner, code, and math model services.
- Added SHACL rule pack aligned to the representation-first mathematical discipline from `AGENTS.md` / `MATH.md`.
- Added schema files for planner, codegen, math reasoning, repair, human checkpoints, run reports, and OpenSearch evidence documents.
- Added 25-word prose chunking extended to the next period or semicolon, while preserving formulas as whole searchable chunks.
- Added APA-style citation generation and metadata propagation into OpenSearch and RDF/Fuseki output.
- Expanded OpenSearch mapping fields for citations, formula metadata, nested formula search, and exact/fuzzy LaTeX search.
- Updated RDF/Turtle generation to include citation, author, year, symbolic shadow IDs, formula descriptions, and formula provenance.
- Quarantined the legacy Python codegen path by making it `generated_unvalidated`; it no longer marks scaffold outputs as `production_candidate_validated`.
- Added API-side SHACL readiness/tool plumbing, run state persistence (`state.json`, `events.jsonl`), command allowlist, and remote OpenAI-compatible fallback path when local vLLM fails and fallback is explicitly enabled.
- Added production-focused tests for chunking, citation, OpenSearch mapping, Turtle/RDF output, and SHACL rule-pack presence.

## What Fuseki receives

Fuseki receives the ontology schema graph and project instance graphs. The ontology graph contains the Veritas OWL-DL vocabulary. Project instance graphs contain documents, chunks, formulas, citations, evidence, plans, risks, source code artifacts, validation results, build artifacts, and human approvals. PDF binaries remain in file storage; Fuseki stores semantic facts and links.

## What OpenSearch receives

OpenSearch receives searchable evidence documents with text fields, keyword identifiers, nested formulas, APA citations, source metadata, normalized vectors, embedding metadata, and chunk/formula records. IDs are `keyword`; prose fields are `text`; formula LaTeX has both analyzed text and exact keyword subfields; embeddings are `knn_vector` with FAISS/HNSW.

## Validation run in this sandbox

Executed successfully:

```bash
python3 -m compileall services/embedding services/ingestion services/shacl
python3 - <<'PY'
import yaml
for f in ['docker-compose.yml','config/veritas.yaml']:
    yaml.safe_load(open(f))
PY
PYTHONPATH=services/ingestion pytest -q tests/ingestion -q
python3 scripts/validate-spec.py
```

Results:

- Python compile: passed
- YAML parse: passed
- Python tests: 14 passed
- Spec validator: ok=true, failed=0, unavailable=2

Unavailable here:

- `cargo check --workspace`: Rust toolchain unavailable in this sandbox.
- `docker compose config` and live Docker E2E: Docker unavailable in this sandbox.

## Host validation required

Run on a real Docker/Rust/GPU host:

```bash
docker compose run --rm cli init
./scripts/bootstrap.sh
docker compose --env-file .veritas/runtime.env --profile models --profile code-model --profile math-model up -d
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
docker compose --env-file .veritas/runtime.env config
docker compose --env-file .veritas/runtime.env run --rm cli ingest-pdf --path tests/fixtures/sample_math_paper.pdf
docker compose --env-file .veritas/runtime.env run --rm cli run "Implement the indexed formula as a tested Rust package" --language rust
```

## Remaining limitations

This pass moves the codebase closer to the target production behavior, but I cannot certify live E2E execution because Docker, Cargo, and GPU/vLLM runtime are unavailable in this environment. The largest remaining production tasks are host validation, Rust compile fixes if any are exposed by `cargo check`, live vLLM structured-output validation, and complete formula-image OCR integration for PDF formula regions.
