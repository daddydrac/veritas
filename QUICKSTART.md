# Veritas Quickstart

This guide starts Veritas with Docker Compose, configures vLLM model routing,
uploads the OWL ontology, ingests research PDFs, and begins prompting the system.

## 1. Prerequisites

Install:

- Docker
- Docker Compose v2
- `curl`
- `jq` recommended

For local vLLM model serving, install NVIDIA Container Toolkit and verify GPU
access:

```bash
docker run --rm --gpus all nvidia/cuda:12.8.0-base-ubuntu24.04 nvidia-smi
```

You can run ingestion/search without local vLLM models, but planning and code
writing are much more useful when a vLLM role is running.

## 2. Configure Veritas

Create defaults:

```bash
cp .env.example .env
```

Interactive setup:

```bash
docker compose run --rm cli init
```

The wizard asks for:

```text
Planner model        default Qwen/Qwen2.5-Coder-7B-Instruct
Code model           default Qwen/Qwen2.5-Coder-14B-Instruct
Code fallback        default deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct
Math model           default allenai/Olmo-3-7B-Instruct or 32B option
Embedding model      default Muennighoff/SBERT-base-nli-v2
Hugging Face token   optional
GPU id               default 0
```

You can paste any Hugging Face model ID supported by vLLM.

## 3. Start core services

```bash
./scripts/bootstrap.sh
```

This starts:

```text
OpenSearch
OpenSearch Dashboards
Jena Fuseki
SBERT embedding service
Veritas API
```

It also uploads the Veritas OWL ontology into Fuseki.

Check readiness:

```bash
curl -s http://localhost:8080/ready | jq
```

## 4. Start vLLM model serving

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

All local model roles:

```bash
docker compose --profile models --profile code-model --profile math-model up -d
```

Show model routing:

```bash
docker compose run --rm cli models
```

## 5. Open the guided CLI startup screen

```bash
docker compose run --rm cli welcome
```

The startup screen prints the Veritas ASCII logo, tagline, service readiness,
knowledge-graph status, model routing, workflow menu, and mode guidance. This is
the intended non-coder entry point.

## 6. Upload or refresh the ontology

```bash
docker compose run --rm cli upload-ontology
```

Upload a custom OWL/RDF/Turtle file:

```bash
docker compose run --rm cli upload-ontology --path ./my-ontology.owl
```

## 7. Ingest arXiv PDFs

```bash
docker compose run --rm cli ingest-arxiv \
  --query "cat:cs.AI OR cat:math.OC" \
  --max-results 3
```

Pipeline:

```text
arXiv search
→ PDF download
→ Docling-first parse
→ formula extraction
→ formula-safe chunks
→ normalized SBERT embeddings
→ OpenSearch FAISS/HNSW index
→ Jena/Fuseki RDF graph upload
```

## 8. Upload a local PDF

```bash
docker compose run --rm cli ingest-pdf --path ./paper.pdf
```

The CLI stages the PDF into `data/papers/uploads/` and runs ingestion inside the
Docker network.

## 9. Search evidence

Hybrid vector + lexical + formula search:

```bash
docker compose run --rm cli search \
  "representation learning invariant structure" \
  --mode hybrid \
  --size 5
```

Lexical/formula search:

```bash
docker compose run --rm cli search "E = mc^2" --mode lexical --size 5
```

## 10. Query the ontology graph

Formula traceability:

```bash
docker compose run --rm cli sparql '
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
SELECT ?formula ?expr ?chunk
WHERE {
  ?formula a veritas:SymbolicShadow ;
           veritas:hasExpressionText ?expr ;
           veritas:derivedFrom ?chunk .
}
LIMIT 20
'
```

## 11. Ask Veritas to plan from research evidence

```bash
docker compose run --rm cli ask \
  "Use the indexed papers to design a tested Rust implementation of the main method."
```

The planner retrieves OpenSearch evidence, queries Fuseki/Jena for formula
traceability, and calls the configured planner vLLM model when available.

## 12. Run autonomous code generation and validation

```bash
docker compose run --rm cli run \
  "Implement the strongest indexed method as a tested package with CPU-safe implementation and GPU extension points." \
  --language rust

# alias that calls the same /run endpoint
docker compose run --rm cli generate-code \
  --language rust \
  --prompt "Implement the strongest indexed method as a tested package with CPU-safe implementation and GPU extension points."
```

The autonomous run creates a workspace under:

```text
data/runs/run-*/
```

The run report includes the files changed, commands run, validation results, retry history, and final status. The generated package status changes to `production_candidate_validated` only when compile/test commands pass. Each workspace includes:

```text
README.md
VALIDATION_REPORT.md
EVIDENCE.md
final_report.json
source files
tests
build/test outputs
```

## 13. Direct model call

```bash
docker compose run --rm cli chat \
  --role planner \
  "Summarize the current evidence-backed implementation plan."
```

Roles:

```text
planner
code
math
```

## 14. Useful logs

```bash
docker compose logs -f api
docker compose logs -f embedding
docker compose logs -f ingestion
docker compose logs -f vllm-planner
docker compose logs -f opensearch
docker compose logs -f fuseki
```

## 15. Stop

```bash
docker compose down
```

Remove data volumes:

```bash
docker compose down -v
```
