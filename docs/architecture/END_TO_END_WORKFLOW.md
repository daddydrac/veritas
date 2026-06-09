# Veritas End-to-End Workflow

This document describes how the architecture supports the user workflow.

## 9. OpenSearch FAISS/HNSW indexing

Veritas indexes each research chunk as a document with:

- text content,
- paper metadata,
- extracted formulas,
- normalized SBERT embedding vector,
- embedding model name,
- embedding norm.

The `embedding` field is an OpenSearch `knn_vector` using FAISS + HNSW and
`cosinesimil` space. Veritas normalizes embeddings before indexing and again
validates query vectors before semantic retrieval.

Benefit: the agent can retrieve mathematically relevant chunks even when the
user does not use the exact words from the paper.

## 10. Jena/Fuseki RDF mapping

Veritas maps each ingested item into RDF:

```text
SourceDocument
  derivedFrom / hasIdentifier / title / hash
RetrievalResult
  derivedFrom SourceDocument
  hasText
  hasOrdinal
SymbolicShadow
  derivedFrom RetrievalResult
  hasExpressionText
  hasConfidenceValue
```

Benefit: formulas are no longer hidden text spans. They become typed semantic
objects that can be queried, traced, validated, and related to generated code.

## 11. SPARQL grounding

Before planning, Veritas runs SPARQL queries such as:

```sparql
PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#>
SELECT ?formula ?expr ?chunk
WHERE {
  ?formula a veritas:SymbolicShadow ;
           veritas:hasExpressionText ?expr ;
           veritas:derivedFrom ?chunk .
}
```

Benefit: the planner can ground its reasoning in ontology facts instead of
hallucinating what formulas, chunks, papers, or risks exist.

## User-visible failure contract

Every production stage must return meaningful failure feedback:

```json
{
  "ok": false,
  "error": {
    "stage": "planning.no_evidence",
    "message": "Veritas could not gather evidence from OpenSearch or Fuseki.",
    "remediation": "Ingest arXiv papers or PDFs first, upload the ontology, then rerun the prompt.",
    "details": {}
  }
}
```
