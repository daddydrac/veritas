# Veritas Vector RAG Validation — OpenSearch FAISS/HNSW + SBERT Cosine

## Requirement

Veritas must index research-paper chunks into OpenSearch using approximate vector search backed by FAISS and HNSW. Chunk and query embeddings must be generated with `Muennighoff/SBERT-base-nli-v2`, normalized to unit length, and evaluated with cosine similarity.

## Implemented design

- OpenSearch image is configured through `VERITAS_OPENSEARCH_VERSION` and defaults to `opensearchproject/opensearch:3.7.0`. The vector index still uses FAISS/HNSW with `space_type: cosinesimil`; Veritas normalizes SBERT vectors before indexing and querying.
- The OpenSearch index mapping creates a `knn_vector` field named `embedding`.
- The vector mapping uses:
  - `engine: faiss`
  - `name: hnsw`
  - `space_type: cosinesimil`
  - `dimension: 768`
  - `m: 24`
  - `ef_construction: 128`
  - `index.knn.algo_param.ef_search: 100`
- The embedding service uses `sentence-transformers` with `Muennighoff/SBERT-base-nli-v2`.
- The embedding service calls `model.encode(..., normalize_embeddings=True)` unless overridden by config.
- Ingestion validates that each stored vector has the configured dimension and norm close to one.
- Search defaults to semantic vector search by embedding the query and issuing an OpenSearch `knn` query.

## Runtime checks

Use these commands after startup:

```bash
veritas start
veritas ready
veritas ingest-arxiv --query "cat:cs.AI OR cat:math.OC" --max-results 3
veritas search "contrastive learning sentence embeddings" --size 5
```

Inspect the OpenSearch mapping:

```bash
curl -s http://localhost:9200/veritas-papers/_mapping | jq '."veritas-papers".mappings.properties.embedding'
```

Expected mapping shape:

```json
{
  "type": "knn_vector",
  "dimension": 768,
  "space_type": "cosinesimil",
  "method": {
    "engine": "faiss",
    "name": "hnsw"
  }
}
```

Check embedding normalization:

```bash
curl -s http://localhost:8090/embed \
  -H 'content-type: application/json' \
  -d '{"texts":["semantic vector retrieval"],"normalize":true}' | jq '.norms'
```

Expected norm:

```json
[1.0]
```

Small floating-point deviation is acceptable within `0.001`.

## Failure modes surfaced to users

- `embedding.request`: embedding service unreachable.
- `embedding.response`: embedding service returns HTTP error.
- `embedding.validate_dimension`: model dimension does not match OpenSearch index dimension.
- `embedding.validate_norm`: embedding is not normalized for cosine search.
- `ingest.index_opensearch`: OpenSearch rejects FAISS/HNSW mapping or indexing request.
- `search.embedding.*`: query embedding failed before vector search.

## Known production caveat

OpenSearch automatically normalizes FAISS/cosinesimil vectors internally in supported versions, but Veritas also normalizes with SBERT before indexing. This keeps behavior explicit, makes cosine evaluation reproducible, and allows fallback to `innerproduct` if a deployment uses an older OpenSearch version.
