# Phase 2 — Real Local Ingestion Backend

Phase 2 replaces Docker-only ingestion assumptions with a real local ingestion backend for the Journey product path.

## Goal

A user can provide a local PDF and receive evidence, formula, citation, RDF, lexical-index, vector-index, review, and ingestion-report artifacts without OpenSearch, Fuseki, or mocked proof scripts.

## Direct command

```bash
PYTHONPATH=services/ingestion \
python3 -m veritas_ingest.cli --config config/veritas.yaml ingest-pdf \
  --path tests/fixtures/sample_math_paper.pdf \
  --backend local \
  --workspace data/runs/local-ingestion-demo
```

## Artifacts

```text
evidence_manifest.json
formula_manifest.json
citation_manifest.json
review_queue.json
chunks.jsonl
formulas.jsonl
citations.jsonl
evidence.ttl
local_lexical_index.jsonl
local_vector_index.jsonl
ingestion_report.md
```

## Embedding behavior

The backend never fabricates embeddings. It uses a real local SentenceTransformer model or explicitly configured HTTP embedding service. If neither is available, it writes all reviewable evidence artifacts and marks planning as `blocked_retrieval_unavailable`.

## Journey behavior

`/journey/run` calls this local backend for local source documents before planning/codegen. If retrieval is unavailable, Journey writes `journey_report.json` and `final_report.json` with `state=blocked_by_retrieval_unavailable` and does not call the planner/codegen loop.
