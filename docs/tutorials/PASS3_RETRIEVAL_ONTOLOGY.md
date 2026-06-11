# Pass 3 Tutorial — Retrieval and Ontology Hardening

Pass 3 makes OpenSearch and Fuseki production resources rather than incidental side effects.

## Business outcome

Veritas must retrieve evidence through a stable OpenSearch FAISS/HNSW vector memory and reason over typed facts in Jena/Fuseki named graphs. A planner should know not only which text chunks are similar, but also which formulas lack invariants, which risks lack mitigation, which plans lack validation, and which generated artifacts are not ready.

## OpenSearch migration flow

Run:

```bash
veritas opensearch-status
veritas opensearch-mapping
veritas opensearch-migrate --dry-run
veritas opensearch-migrate
```

The migration creates a versioned index and aliases:

```text
base index:       veritas-papers
versioned index:  veritas-papers-v1
read alias:       veritas-papers-read
write alias:      veritas-papers-write
```

OpenSearch fields are intentionally typed:

```text
keyword: doc_id, chunk_id, formula_id, run_id, status, sha256
text: title, abstract, chunk_text, formula_description, technical_summary
nested: formulas, citations
knn_vector: embedding, formula_embedding
```

If an alias is missing, search attempts configured fallback targets and reports the failed targets instead of hiding the issue.

## Fuseki named graph flow

Fuseki receives RDF facts, not PDF binaries.

```text
urn:veritas:graph:ontology              ontology TBox
urn:veritas:graph:document:<hash>       document ABox facts
urn:veritas:graph:run:<run_id>          run/source-code facts
urn:veritas:graph:validation:<run_id>   validation facts
```

Commands:

```bash
veritas graph-list
veritas graph-facts
veritas graph-describe urn:veritas:graph:ontology
veritas graph-upload --path ./facts.ttl --graph-uri urn:veritas:graph:document:example
```

## Planner fact summary

The API loads the production SPARQL query pack and sends a compact fact summary to the planner. This includes formulas without invariants, risks without mitigation, plans without validation, unvalidated source artifacts, builds without tests, loops without termination, objectives blocked by assumptions, deployment units without observability, and math claims without transfer tests.

This preserves the Veritas design principle: retrieval provides evidence; ontology provides obligations.
