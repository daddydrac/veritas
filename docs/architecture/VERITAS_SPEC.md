# Veritas Production Specification

**Veritas** is a math-heavy, evidence-backed research and development software-engineering agent.

The system is validated against the architecture diagram with these planes:

1. **Users & Stakeholders** — mathematical researcher, architect, software engineer, DevOps/SRE.
2. **Specialist Agents** — ontology cartographer, representation cartographer, invariant miner, software engineering agent, DevOps/SRE agent, risk and validation agent, mathematical research agent.
3. **Agent Orchestrator / Planner** — intent understanding, task planning, tool/model routing, context assembly, response synthesis.
4. **Semantic Reasoning Plane** — Veritas OWL-DL ontology, BFO/CCO alignment, Jena/Fuseki triple store, SPARQL, OWL reasoning, SHACL/rule validation.
5. **Retrieval & Memory Plane** — OpenSearch lexical retrieval, vector memory, property/dependency graph, document/code corpus, logs/traces/metrics.
6. **Execution & Analysis Plane** — plan synthesis, control-flow back-check, risk assessment, proof/transfer check, simulation/analysis, action execution, Formula/LaTeX understanding, math-to-code synthesis, distributable code packaging.
7. **Cross-Domain Shared Concepts** — objective, plan, task, capability, constraint, risk, invariant, evidence, validation, observable signal.
8. **Outputs** — research notes, hypotheses, architecture decisions, implementation plans, runbooks, deployment plans, executable models, distributable packages, validated findings.

## Required user workflow

The target non-developer workflow is:

```text
1. Start Veritas from the CLI.
2. Configure service/model/index/ontology/chunking knobs through prompts or config files.
3. Upload local arXiv PDFs or ingest directly from arXiv search.
4. Veritas parses PDFs with a math-aware parser.
5. Veritas extracts formulas, chunks text without splitting formulas, and records formula context.
6. Veritas writes chunks to OpenSearch for search/RAG.
7. Veritas writes source documents, chunks, and symbolic shadows to Jena/Fuseki as RDF.
8. Veritas runs ontology-guided SPARQL queries to ground planning.
9. Veritas generates evidence-backed research summaries and representation hypotheses.
10. Veritas turns math/formulas into production code plans.
11. Veritas generates maintainable CPU/GPU distributable code packages.
12. Veritas validates tests, risks, control flow, evidence, and deployment readiness.
13. Veritas returns clear results or meaningful failure messages with exact failure stage and remediation.
```

## Required task outcomes

| Task | Required outcome | Current scaffold status |
|---|---|---|
| Start stack | `veritas start` prints logo, creates `.env`, starts Docker Compose, and prints next steps. | Implemented in CLI scaffold. |
| Health feedback | `veritas ready` probes API dependencies and reports failed service and remediation. | Implemented for API, OpenSearch, Fuseki. |
| arXiv ingestion | CLI loads PDFs from arXiv query into OpenSearch and Fuseki. | Implemented scaffold. |
| Local PDF upload | CLI stages a local PDF into Docker volume and ingests it. | Implemented scaffold. |
| PDF parsing | Docling-first conversion with pypdf fallback. | Implemented scaffold; formula accuracy must be corpus-validated. |
| Formula extraction | Preserve formula body, raw expression, offsets, context, source, pattern, and confidence. | Implemented heuristic extraction. |
| Formula-safe chunking | Do not split formula spans across chunks; preserve context. | Implemented with smoke tests. |
| OpenSearch indexing | Index text, metadata, formulas, and formula fields. | Implemented. |
| Jena/Fuseki mapping | Upload source documents, chunks, and formulas as RDF. | Implemented. |
| Cross-domain SPARQL | Query graph for evidence chunks and formula traceability. | Implemented sample queries. |
| OWL-DL reasoning | Offline Openllet reasoner container. | Implemented command wrapper; not fully CI-proven in sandbox. |
| Vector memory | Qdrant service available for future RAG embeddings. | Service added; embeddings not wired. |
| Property graph | Dependency/execution graph. | Not implemented. |
| SHACL governance | Closed-world validation rules. | Not implemented. |
| Agent planner | Evidence-backed planner and model routing. | `/plan` is still deterministic scaffold. |
| Math-to-code | Formula/LaTeX to production-grade code. | Not implemented beyond plan envelope. |
| GPU distribution | CPU/GPU distributable code generation. | GPU runtime scaffold exists; code generation/distribution not implemented. |
| Novel math discovery | Representation-first discovery workflow. | Spec present; agent implementation not complete. |

## Failure message contract

Every CLI/API/worker failure must include:

```json
{
  "ok": false,
  "error": {
    "stage": "where.failure.happened",
    "message": "what failed in human language",
    "remediation": "what the user should do next",
    "details": {}
  }
}
```

Required examples:

- `ingest.search_arxiv`: no papers matched the query.
- `ingest.validate_pdf`: PDF path missing, not readable, or empty.
- `ingest.chunk_pdf`: parser produced no text/chunks.
- `ingest.index_opensearch`: OpenSearch indexing failed.
- `ingest.upload_fuseki`: RDF graph upload failed.
- `api.ready`: one or more dependencies are unreachable.
- `api.sparql`: Fuseki query failed.
- `api.search`: OpenSearch query failed.
- `api.plan`: missing goal or production planner not wired.

## Production acceptance gates

Veritas is production-ready only when all gates pass:

```text
[ ] Docker Compose starts from a clean checkout with one command.
[ ] CLI logo and first-run prompts appear.
[ ] arXiv ingest completes for at least one math-heavy paper.
[ ] Local PDF ingest completes for at least one fixture.
[ ] OpenSearch contains searchable chunks with formulas.
[ ] Fuseki contains RDF triples for source documents, chunks, and formulas.
[ ] SPARQL traceability query returns formulas and source chunks.
[ ] OWL-DL reasoner validates ontology consistency.
[ ] SHACL validates evidence, risk, and control-flow completeness.
[ ] Planner runs retrieval + SPARQL before generating any code plan.
[ ] Generated code includes tests, README, runtime spec, and validation report.
[ ] CPU/GPU package target is selected from config, not hard-coded.
[ ] Failures return stage, message, remediation, and details.
```
