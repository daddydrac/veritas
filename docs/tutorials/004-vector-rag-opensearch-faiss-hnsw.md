# Tutorial 004 — Vector RAG with OpenSearch FAISS/HNSW and SBERT

## Goal

Configure Veritas so every research chunk is searchable by semantic similarity using normalized SBERT embeddings and OpenSearch FAISS/HNSW.

## Why this matters

Mathematical research chunks often use different surface language for the same idea. Lexical search can miss relevant evidence. Vector search lets Veritas retrieve chunks by semantic meaning, while the ontology and SPARQL layer ground retrieved evidence in typed facts.

## Configuration

The default embedding model is:

```text
Muennighoff/SBERT-base-nli-v2
```

The model produces 768-dimensional vectors. Veritas normalizes all vectors so cosine similarity is equivalent to dot product over unit vectors.

OpenSearch vector settings live in `config/veritas.yaml`:

```yaml
services:
  opensearch:
    vector:
      enabled: true
      field: embedding
      engine: faiss
      method: hnsw
      space_type: cosinesimil
      dimension: 768
      m: 24
      ef_construction: 128
      ef_search: 100
```

## Run

```bash
veritas start
veritas ready
veritas ingest-arxiv --query "cat:cs.AI OR cat:math.OC" --max-results 3
veritas search "representation learning invariant structure" --size 5
```

## Acceptance criteria

- [ ] OpenSearch starts on version 2.19+.
- [ ] The `veritas-papers` index has `index.knn=true`.
- [ ] The `embedding` field is `knn_vector` with `engine=faiss` and `name=hnsw`.
- [ ] The embedding service reports `dimension=768`.
- [ ] Ingested chunks contain `embedding`, `embedding_model`, and `embedding_norm`.
- [ ] `embedding_norm` is within `0.001` of `1.0`.
- [ ] `/search` defaults to semantic k-NN retrieval.

## Troubleshooting

If ingestion fails with `embedding.validate_dimension`, the model dimension and OpenSearch mapping disagree. Keep `Muennighoff/SBERT-base-nli-v2` with dimension `768`, or recreate the index after changing models.

If ingestion fails with `embedding.validate_norm`, ensure `VERITAS_EMBEDDING_NORMALIZE=true` and inspect `docker compose logs embedding`.
