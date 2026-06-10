# Validation Run

Date: 2026-06-09

## Local validation completed in sandbox

Passed:

```text
Python ingestion tests: 9 passed
Python module compilation: passed
docker-compose.yml YAML parse: passed
config/veritas.yaml YAML parse: passed
OWL/RDF parse: passed
Formula extraction smoke test: passed
Formula-preserving chunk boundary test: passed
Generated Turtle parse: passed
Veritas spec validation: ok=true, failed=0
```

Unavailable in this sandbox:

```text
Docker Compose runtime startup
Cargo / Rust compilation
CUDA / GPU runtime validation
Live vLLM model loading
Live OpenSearch/Fuseki/Qdrant E2E test
```

## Notes

The model serving layer is configured for vLLM OpenAI-compatible endpoints. Rust API and CLI components call vLLM over HTTP; vLLM owns Hugging Face model download/cache/runtime execution. The default vLLM image is `vllm/vllm-openai:latest`, matching the official Docker documentation's stable example pattern, and can be pinned with `VERITAS_VLLM_IMAGE`.
