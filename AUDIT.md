# Veritas audit

This audit reflects the current codebase after replacing the deterministic planning path with a vLLM-backed autonomous planning, coding, and validation loop.

## Resolved gaps

### 1. `/plan` was a deterministic envelope

Status: resolved in source.

Implementation:

- `apps/api/src/main.rs` now builds a structured plan through the configured vLLM planner role.
- The planner prompt requires JSON only.
- Veritas parses the model response as JSON, attempts one JSON repair pass, and validates the result against the Veritas plan schema.
- If vLLM is unavailable or the schema is invalid, `/plan` returns a meaningful failure instead of silently returning a deterministic fallback.

### 2. No actual autonomous `/run` loop

Status: resolved in source.

Implementation:

- `POST /run` creates a run workspace under `VERITAS_RUNS_DIR`.
- The run loop calls `/plan` internals, executes planner-selected retrieval/SPARQL/math tools, routes to the code model, writes real files, runs compile/test commands, feeds failures back to the code model, retries with a bounded limit, and writes `final_report.json`.
- The final report includes original task, plan, model routes, tool calls, files changed, commands run, validation results, retry history, generated package status, and remaining limitations.

### 3. No actual math-to-code generator

Status: resolved in source for model-backed package generation.

Implementation:

- Math-to-code is now executed through `/run` and `veritas run` / `veritas generate-code`.
- The code model must return a JSON object containing complete files and validation commands.
- Generated files are written into the run workspace and validated by compile/test commands.
- Package status changes to `production_candidate_validated` only if validation commands pass.

### 4. No multi-GPU code distribution layer; only optional Ollama existed

Status: partially resolved by removing unsupported claims and wiring vLLM GPU routing.

Implementation:

- Ollama is no longer the default path.
- vLLM services exist for planner, code, and math roles.
- Docker Compose exposes GPU assignment, tensor-parallel, pipeline-parallel, max context, dtype, and GPU memory utilization variables per role.
- Veritas no longer claims generated application code has real CUDA/Candle/Burn/wgpu execution unless the model generates and tests that code in the run workspace.

Remaining production validation:

- Live multi-GPU vLLM loading must be validated on a GPU host.

### 5. No vector index service wired

Status: resolved in source.

Implementation:

- OpenSearch is the default vector index.
- Qdrant was removed.
- Ingestion creates OpenSearch FAISS/HNSW `knn_vector` mappings.
- Embeddings use normalized SBERT vectors for cosine-compatible retrieval.
- Retrieval is used by `/plan` and `/run` before making claims.

### 6. Property graph service referenced but not implemented

Status: resolved by architecture correction.

Implementation:

- The property graph claim was removed from the active implementation path.
- The implemented graph layer is Jena Fuseki with RDF/SPARQL and the Veritas OWL ontology.

### 7. SHACL gap

Status: intentionally skipped by user direction.

Implementation:

- No SHACL container or rule pack is included in this iteration.
- Closed-world governance can be reintroduced later, but the current requirement explicitly said to skip it.

### 8. Docling formula extraction best-effort

Status: mitigated, not eliminated.

Implementation:

- Docling-first extraction remains.
- Regex fallback remains and is documented as not mathematically complete.
- Formula-preserving chunk tests and a local sample PDF golden assertion exist.

Remaining production validation:

- Formula quality must be validated per target corpus, especially for scanned PDFs, dense theorem papers, or unusual LaTeX rendering.

### 9. Cargo checks not previously runnable

Status: not executed in this sandbox.

Reason:

- `cargo` and `rustc` are unavailable in the current execution environment.

Required host command:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### 10. Docker E2E not previously runnable

Status: not executed in this sandbox.

Reason:

- Docker is unavailable in the current execution environment.

Required host commands:

```bash
docker compose config
./scripts/bootstrap.sh
docker compose --profile models --profile code-model --profile math-model up -d
docker compose run --rm cli ready
docker compose run --rm cli ingest-pdf --path tests/fixtures/sample_math_paper.pdf
docker compose run --rm cli run "Implement the indexed formula as a tested Rust package" --language rust
```

## Current acceptance status

| Requirement | Status |
|---|---|
| `/run` creates a run workspace | Implemented |
| Planner returns JSON only and schema is validated | Implemented |
| Loop executes planner-selected tools | Implemented for retrieval, SPARQL, math, codegen, tests |
| Code model writes real files | Implemented |
| Compile/test commands run | Implemented in `/run` |
| Test failures are fed back to code model | Implemented |
| Bounded retry loop | Implemented |
| Final report includes files changed and commands run | Implemented |
| Package status changes only after tests pass | Implemented |
| Unsupported GPU runtime claims removed | Implemented |
| Docker Compose E2E passes | Not run in this sandbox; host validation required |

## Remaining limitations

1. Live vLLM model loading was not validated in this environment.
2. Docker Compose E2E was not validated in this environment.
3. Rust compile/test was not validated in this environment.
4. Formula extraction quality remains corpus-dependent.
5. The generated code quality depends on the selected vLLM model and the quality of retrieved evidence.
