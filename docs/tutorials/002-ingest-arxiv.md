# Tutorial 002 — Ingest arXiv PDFs

## Goal
Download papers from arXiv, parse with Docling, preserve formulas, chunk text, index OpenSearch, and update Jena.

## Steps

```bash
./scripts/ingest-demo.sh "cat:cs.AI OR cat:math.OC" 3
```

## What happens

1. arXiv Atom API search.
2. PDF download.
3. Docling conversion to Markdown/JSON.
4. Regex/context formula pass.
5. Formula-preserving chunking.
6. OpenSearch indexing.
7. Turtle RDF generation.
8. Fuseki graph upload.

## Acceptance Criteria

- `data/papers/` contains PDFs.
- `data/docling/` contains Docling outputs.
- `data/chunks/` contains JSONL chunks and latest-ingest TTL.
- OpenSearch search returns paper chunks.
- SPARQL query returns `SymbolicShadow` formula facts.
