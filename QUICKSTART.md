# Veritas Quickstart

This guide starts Veritas with Docker Compose, uploads the ontology, ingests
research PDFs, and begins prompting the system.

## 1. Prerequisites

Install:

- Docker
- Docker Compose v2
- `curl`
- `jq` recommended

For GPU-backed local LLM profiles, install NVIDIA Container Toolkit and verify:

```bash
docker run --rm --gpus all nvidia/cuda:12.6.0-base-ubuntu22.04 nvidia-smi
```

## 2. Configure

```bash
cp .env.example .env
```

Most knobs are dynamic:

```text
.env                    service ports, model names, GPU id
config/veritas.yaml     chunking, ontology graph URIs, vector settings, codegen
packages/ontology/      OWL ontology and SPARQL queries
```

Default embedding model:

```text
Muennighoff/SBERT-base-nli-v2
```

## 3. Start the full stack

```bash
./scripts/bootstrap.sh
```

This starts:

```text
OpenSearch
OpenSearch Dashboards
Jena Fuseki
Qdrant placeholder service
SBERT embedding service
Veritas API
```

It also uploads the Veritas OWL ontology into Fuseki.

Check readiness:

```bash
curl -s http://localhost:8080/ready | jq
```

## 4. Start with Docker Compose only

Equivalent manual commands:

```bash
docker compose up -d --build opensearch fuseki qdrant embedding api

docker compose run --rm ingestion \
  python -m veritas_ingest.cli upload-ontology
```

CLI container:

```bash
docker compose run --rm cli welcome
```

## 5. Ingest arXiv PDFs

```bash
docker compose run --rm cli ingest-arxiv \
  --query "cat:cs.AI OR cat:math.OC" \
  --max-results 3
```

What happens:

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

## 6. Upload a local PDF

```bash
docker compose run --rm cli ingest-pdf --path ./paper.pdf
```

The CLI stages the PDF into `data/papers/uploads/` and runs ingestion inside the
Docker network.

## 7. Search evidence

Semantic vector search:

```bash
docker compose run --rm cli search "representation learning invariant structure" --size 5
```

Lexical/formula search through API:

```bash
curl -s http://localhost:8080/search \
  -H 'content-type: application/json' \
  -d '{"query":"E = mc^2", "mode":"lexical", "size":5}' \
  | jq
```

## 8. Query the ontology graph

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

## 9. Ask Veritas to plan from research evidence

```bash
docker compose run --rm cli ask \
  "Use the indexed papers to design a tested Rust implementation of the main method."
```

The planner retrieves OpenSearch evidence, queries Fuseki/Jena for formula
traceability, and emits an evidence-backed plan with risks and validation gates.

## 10. Generate a review-gated code package

```bash
docker compose run --rm cli generate-code \
  --language rust \
  --prompt "Implement the strongest indexed method as a tested package with CPU/GPU extension points."
```

Generated packages land in:

```text
data/generated/
```

Each package includes:

```text
README.md
VALIDATION_REPORT.md
EVIDENCE.md
veritas_manifest.json
source files
tests
```

## 11. Stop Veritas

```bash
docker compose down
```

Destroy volumes if you want a clean reset:

```bash
docker compose down -v
```

## Troubleshooting

Check all services:

```bash
docker compose ps
curl -s http://localhost:8080/ready | jq
```

Logs:

```bash
docker compose logs --tail=200 opensearch
docker compose logs --tail=200 fuseki
docker compose logs --tail=200 embedding
docker compose logs --tail=200 api
```

Common issue: embedding model download is slow on first boot. Wait for the
embedding service to become healthy, then retry ingestion.
