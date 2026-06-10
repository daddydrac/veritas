# Production E2E Audit

Veritas now includes source-level implementation for the autonomous workflow: structured planner JSON, planner-selected tools, run workspace creation, vLLM code generation, file writing, compile/test command execution, bounded retries, and final reports.

## Implemented services

```text
Rust API
Rust CLI
vLLM planner service
vLLM code service
vLLM math service
OpenSearch FAISS/HNSW
Jena Fuseki RDF/SPARQL
Openllet reasoner container
SBERT embedding service
Docling-first ingestion worker
```

## Not included by current product direction

```text
Qdrant
Property graph service
SHACL container/rule pack
Ollama default path
```

## Host validation required

The current sandbox cannot run Docker, Cargo, or GPU workloads. Run this on the target host:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
docker compose config
./scripts/bootstrap.sh
docker compose --profile models --profile code-model --profile math-model up -d
docker compose run --rm cli ready
docker compose run --rm cli ingest-pdf --path tests/fixtures/sample_math_paper.pdf
docker compose run --rm cli run "Implement the indexed formula as a tested Rust package" --language rust
```
