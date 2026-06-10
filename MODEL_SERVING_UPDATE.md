# Veritas Model Serving Update

## Summary

Veritas now treats **vLLM** as the model serving solution. Rust does not load
Hugging Face weights directly. The Rust API and CLI call vLLM's OpenAI-compatible
HTTP endpoints, while vLLM downloads and serves Hugging Face models from the
shared `hf-cache` Docker volume.

## Default models

```text
Planner:          Qwen/Qwen2.5-Coder-7B-Instruct
Code primary:     Qwen/Qwen2.5-Coder-14B-Instruct
Code fallback:    deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct
Math default:     allenai/Olmo-3-7B-Instruct
Math large:       allenai/Olmo-3.1-32B-Instruct
Embeddings:       Muennighoff/SBERT-base-nli-v2
Ontology:         Jena Fuseki + Openllet
```

## New commands

```bash
veritas init
veritas models
veritas chat --role planner "Create an implementation plan"
veritas start --models
veritas start --models --code-model --math-model
```

## New API endpoints

```text
GET  /models
POST /llm/chat
```

## Docker Compose profiles

```text
models      -> vllm-planner
code-model  -> vllm-code
math-model  -> vllm-math
```

## Validation

Static validation performed in this environment:

```text
Python compile: passed
YAML parse: passed
RDF/OWL parse: passed
Unit tests: 9 passed
scripts/validate-spec.py: passed, with cargo/docker unavailable in sandbox
```

Docker, Cargo, vLLM, and GPU runtime validation must be performed on a host with
Docker, Rust toolchain, and NVIDIA Container Toolkit.
