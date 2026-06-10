# Veritas Model Routing

Veritas uses **vLLM** as the local model serving solution. The Rust API and CLI do
not load Hugging Face models directly. They call vLLM's OpenAI-compatible HTTP
API, and the vLLM containers download/cache models from Hugging Face.

## Default roles

| Role | Default model | Served name | Service |
|---|---|---|---|
| Planner | `Qwen/Qwen2.5-Coder-7B-Instruct` | `veritas-planner` | `vllm-planner` |
| Code writer | `Qwen/Qwen2.5-Coder-14B-Instruct` | `veritas-code` | `vllm-code` |
| Code fallback | `deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct` | configurable | used in prompts/config |
| Math reasoning | `allenai/Olmo-3-7B-Instruct` | `veritas-math` | `vllm-math` |
| Math large | `allenai/Olmo-3.1-32B-Instruct` | configurable | optional replacement |
| Embeddings | `Muennighoff/SBERT-base-nli-v2` | n/a | `embedding` |
| Ontology reasoning | Openllet + Jena SPARQL | n/a | `reasoner`, `fuseki` |

## Configure

```bash
docker compose run --rm cli init
```

The wizard writes `.env` with the selected Hugging Face model IDs.

## Start vLLM services

Planner only:

```bash
docker compose --profile models up -d vllm-planner
```

Code writer:

```bash
docker compose --profile code-model up -d vllm-code
```

Math reasoner:

```bash
docker compose --profile math-model up -d vllm-math
```

All roles:

```bash
docker compose --profile models --profile code-model --profile math-model up -d
```

## Inspect routing

```bash
docker compose run --rm cli models
curl -s http://localhost:8080/models | jq
```

## Direct chat call

```bash
docker compose run --rm cli chat --role planner \
  "Create an evidence-backed implementation plan from indexed papers."
```

Valid roles:

```text
planner
code
math
```

## Hardware notes

The default code model is 14B and the optional math model is 32B. Those are not
small-GPU defaults. On a small workstation GPU, use:

```text
Planner: Qwen/Qwen2.5-Coder-7B-Instruct
Code:    Qwen/Qwen2.5-Coder-7B-Instruct
Math:    allenai/Olmo-3-7B-Instruct
```

For larger models, configure a remote OpenAI-compatible endpoint or use a machine
with enough VRAM.
