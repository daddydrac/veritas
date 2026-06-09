# Veritas

**Math heavy evidence backed research and development software engineering agent.**

Veritas is an open-source, Docker-first agentic system for turning math-heavy
research into auditable engineering plans, searchable evidence, and
review-gated distributable code packages. It combines PDF ingestion, formula
extraction, OpenSearch FAISS/HNSW vector RAG, Jena/Fuseki ontology graphs,
SPARQL grounding, and representation-first math analysis.

```text
██╗   ██╗███████╗██████╗ ██╗████████╗ █████╗ ███████╗
██║   ██║██╔════╝██╔══██╗██║╚══██╔══╝██╔══██╗██╔════╝
██║   ██║█████╗  ██████╔╝██║   ██║   ███████║███████╗
╚██╗ ██╔╝██╔══╝  ██╔══██╗██║   ██║   ██╔══██║╚════██║
 ╚████╔╝ ███████╗██║  ██║██║   ██║   ██║  ██║███████║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚══════╝
```

## What problem does Veritas solve?

Research-to-production work usually loses the reasoning chain:

```text
paper → formulas → assumptions → implementation plan → code → tests → deployment
```

Veritas preserves that chain. It maps papers, chunks, formulas, evidence,
risks, assumptions, and generated artifacts into a semantic graph so agents can
query and ground their planning before producing code.

## What Veritas does today

The current system provides an automated foundation for the intended workflow:

1. Start with Docker Compose.
2. Print the Veritas logo and tagline through the CLI.
3. Configure services, models, chunking, graph URIs, and codegen through
   `.env` and `config/veritas.yaml`.
4. Ingest arXiv PDFs or local PDFs.
5. Parse PDFs with a Docling-first pipeline and pypdf fallback.
6. Extract LaTeX formulas and preserve formula context.
7. Chunk text without splitting formulas.
8. Embed chunks with normalized SBERT vectors using
   `Muennighoff/SBERT-base-nli-v2`.
9. Index text, formulas, and vectors into OpenSearch FAISS/HNSW.
10. Map papers, chunks, and formulas into Jena/Fuseki RDF.
11. Run SPARQL over the ontology graph to ground planning.
12. Retrieve evidence before making claims.
13. Produce a representation-first math-analysis plan.
14. Generate a review-gated package scaffold from research evidence.
15. Emit validation gates, risks, assumptions, and failure envelopes.
16. Return meaningful success or failure messages with remediation steps.

## Architecture

```text
User / CLI / API
  → Agent Orchestrator / Planner
  → Retrieval & Memory Plane
      OpenSearch FAISS/HNSW + normalized SBERT embeddings
  → Semantic Reasoning Plane
      Jena/Fuseki + Veritas OWL-DL ontology + SPARQL
  → Execution & Analysis Plane
      risk, validation, control-flow, code package generation
  → Outputs
      research notes, implementation plans, generated packages, findings
```

## Key technologies

- **Docker Compose** for one-command local deployment.
- **OpenSearch 2.19.5** with FAISS/HNSW `knn_vector` fields.
- **SentenceTransformers** using `Muennighoff/SBERT-base-nli-v2`.
- **Normalized embeddings** for cosine similarity.
- **Apache Jena Fuseki** for RDF/SPARQL graph storage.
- **Veritas OWL-DL ontology** for cross-domain reasoning.
- **Docling-first PDF parsing** with formula-preserving fallback extraction.
- **Rust API and CLI** for service orchestration.
- **Python ingestion workers** for document processing and package generation.

## Repository layout

```text
apps/
  api/                     Rust API service
  cli/                     Rust CLI
services/
  ingestion/               PDF, formula, embedding, RDF, OpenSearch pipeline
  embedding/               SBERT embedding HTTP service
  reasoner/                Openllet offline reasoner container
packages/
  ontology/                Veritas OWL ontology and SPARQL queries
config/
  veritas.yaml             Main dynamic configuration
scripts/
  bootstrap.sh             Fully automated local startup
  ingest-demo.sh           arXiv ingestion helper
  upload-ontology.sh       Fuseki ontology upload helper
  generate-code.sh         Evidence-backed package generation helper
docs/
  tutorials/               Task-based technical tutorials
  architecture/            System spec and workflow notes
  validation/              Validation reports
```

## Quickstart

See [QUICKSTART.md](QUICKSTART.md).

## Core commands

```bash
cp .env.example .env
./scripts/bootstrap.sh
./scripts/ingest-demo.sh "cat:cs.AI OR cat:math.OC" 3

docker compose run --rm cli search "invariant representation"
docker compose run --rm cli ask "turn indexed research into tested Rust code"
docker compose run --rm cli generate-code \
  --language rust \
  --prompt "implement the strongest indexed method as a tested package"
```

## Ontology reasoning

Upload the ontology:

```bash
./scripts/upload-ontology.sh
```

Run a SPARQL query:

```bash
curl -s http://localhost:8080/sparql \
  -H 'content-type: application/json' \
  -d '{"query":"PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#> SELECT ?formula ?expr WHERE { ?formula a veritas:SymbolicShadow ; veritas:hasExpressionText ?expr . } LIMIT 20"}' \
  | jq
```

## OpenSearch vector RAG

Veritas indexes evidence chunks with this vector shape:

```json
{
  "type": "knn_vector",
  "dimension": 768,
  "space_type": "cosinesimil",
  "method": {
    "name": "hnsw",
    "engine": "faiss"
  }
}
```

Embeddings are generated with `normalize_embeddings=True` and validated for
unit L2 norm before indexing and querying.

## Failure behavior

Veritas should fail loudly and usefully. Failures are JSON envelopes with:

```json
{
  "ok": false,
  "error": {
    "stage": "ingest.embed_chunks",
    "message": "What failed",
    "remediation": "How to fix it",
    "details": {}
  }
}
```

## Production note

Generated packages are **review-gated**. Veritas generates source, tests,
evidence summaries, and validation reports, but theorem-level correctness and
production release still require the validation gates to pass.

## Contributing

Contributions should preserve:

- configuration over hard-coded strings,
- meaningful error envelopes,
- formula-preserving chunking,
- normalized vector search,
- ontology namespace consistency,
- testable modules,
- evidence-backed planning.

Run checks:

```bash
make validate
PYTHONPATH=services/ingestion pytest -q tests/ingestion
```
