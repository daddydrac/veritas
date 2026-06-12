# Phase 4 — Source-Mocked Retrieval and Ontology Hardening

Phase 4 raises Veritas retrieval and ontology features to A/A- source-level confidence without requiring live OpenSearch or live Fuseki in restricted CI/sandbox environments.

Live OpenSearch/Fuseki validation remains a host-only acceptance gate. This phase proves the contracts that the live services must obey.

## What Phase 4 proves

```text
OpenSearch mapping contract
OpenSearch FAISS/HNSW vector fields
keyword/text/nested field discipline
vector dimension mismatch rejection
versioned index + read/write alias migration behavior
search fallback across read alias, write alias, base index, versioned index
Fuseki named graph discipline
Graph-store upload request construction
PDF binaries are not uploaded to RDF graphs
run-report RDF facts include SourceCodeArtifact, VerificationResult, BuildArtifact
SPARQL query-pack summary covers all planner grounding queries
```

## Source-mocked proof command

```bash
scripts/e2e/source-mocked-retrieval-ontology.sh
```

Expected outcome:

```text
ok=true
opensearch_mapping_contract=true
vector_dimension_mismatch_rejected=true
opensearch_migration_alias_update=true
opensearch_migration_idempotent_second_run=true
opensearch_retrieval_fallback_read_to_write=true
fuseki_named_graphs_distinct=true
fuseki_graph_store_upload_contract=true
fuseki_rejects_pdf_binary_payload=true
run_report_rdf_contains_source_build_validation=true
planner_sparql_fact_summary_all_queries=true
```

The script writes:

```text
data/e2e/source-mocked-retrieval-ontology/phase4-summary.json
```

## OpenSearch contract

The OpenSearch evidence index must use:

```text
keyword: IDs, hashes, status, model names, source type
text: title, abstract, chunk text, formula descriptions, summaries
nested: formulas, citations, validation results
knn_vector: chunk embedding and formula embedding
engine: faiss
method: hnsw
space_type: cosinesimil
```

The source-level contract lives in:

```text
services/ingestion/veritas_ingest/retrieval_ontology_contracts.py
```

The Rust API mapping lives in:

```text
apps/api/src/main.rs::production_opensearch_mapping
```

## Fuseki named graph contract

Fuseki stores semantic RDF facts, not PDF binaries.

Named graphs are separated by concern:

```text
urn:veritas:graph:ontology
urn:veritas:graph:document:<document_hash>
urn:veritas:graph:run:<run_id>
urn:veritas:graph:validation:<run_id>
```

The document graph contains facts such as:

```text
SourceDocument
RetrievalResult
SymbolicShadow
APA citation metadata
formula metadata
embedding metadata
```

The run graph contains facts such as:

```text
PlannedEngineeringAct
SourceCodeArtifact
BuildArtifact
VerificationResult
human checkpoint facts
risk/validation status facts
```

## Planner SPARQL facts

Phase 4 validates that the planner fact summary covers the query pack:

```text
formula_traceability
evidence_chunks
formulas_without_invariants
risks_without_mitigation
plans_without_validation
unvalidated_code_artifacts
builds_without_tests
loops_without_termination
objectives_blocked_by_assumptions
deployment_units_without_observability
math_claims_without_transfer_tests
```

These query results are summarized into compact JSON before they are given to the planner.

## Business outcome

Phase 4 ensures retrieval and ontology are not incidental side effects. OpenSearch becomes a stable evidence memory. Fuseki becomes the semantic reasoning substrate. The planner receives typed, cross-domain facts instead of raw text alone, and Veritas preserves the chain from research evidence to formulas, risks, plans, code, validation, and build artifacts.
