# Veritas scaffold audit

This scaffold is runnable as an infrastructure foundation, but it is not yet the complete autonomous research-to-production agent. The audit fixed two E2E blockers: formula/chunk boundary handling and Turtle escaping for LaTeX RDF upload.

## Fixed in this audited package

- RDF/Turtle generation now uses rdflib Literals instead of hand-escaped strings, so LaTeX backslashes, quotes, unicode, and newlines do not corrupt graph upload.
- Formula-preserving chunking no longer associates formulas with chunks that only contain partial formula text.
- OpenSearch development container now includes `DISABLE_INSTALL_DEMO_CONFIG` and an initial admin password env for newer images.
- Fuseki healthcheck now uses the admin ping endpoint instead of a SPARQL endpoint without a query.
- API now propagates upstream OpenSearch/Fuseki HTTP status codes instead of always returning 200 for upstream errors.

## Still scaffold / not complete

- `/plan` is a deterministic envelope, not a real model-router/agent planner.
- No actual math-to-code generator is implemented yet.
- No multi-GPU code distribution layer is implemented yet; only an optional Ollama GPU service exists.
- No vector index service is wired despite architecture references to FAISS/Qdrant/HNSW.
- No property graph service is wired despite architecture references.
- No SHACL engine is wired yet.
- Docling formula extraction is best-effort and must be validated per corpus; the regex fallback is useful but not mathematically complete.
- Rust cargo checks could not be run in this sandbox because cargo is not installed.
- Docker E2E could not be run in this sandbox because Docker is not available.

## Suggested next implementation tickets

1. Add integration tests with docker compose health waits.
2. Add a local sample PDF fixture and golden extracted-formula assertions.
3. Add Qdrant or FAISS vector memory service.
4. Add SHACL validation container and rule pack.
5. Replace `/plan` with real orchestrator + model provider abstraction.
6. Add math-to-code worker with generated package validation.
7. Add GPU backend abstraction for CUDA/Candle/Burn/wgpu.
