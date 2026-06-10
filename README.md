# Veritas

**Math-heavy evidence-backed research and development software engineering agent.**

Veritas is an open-source, Docker-first agentic system for turning math-heavy
research into auditable engineering plans, searchable evidence, ontology-grounded
analysis, and review-gated distributable code packages. It combines PDF ingestion,
formula extraction, OpenSearch FAISS/HNSW vector RAG, Jena/Fuseki ontology graphs,
SPARQL grounding, vLLM model serving, and representation-first mathematical
analysis.

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
paper → formulas → assumptions → invariants → implementation plan → code → tests → deployment
```

Veritas preserves that chain. It maps papers, chunks, formulas, evidence, risks,
assumptions, and generated artifacts into a semantic graph so agents can query and
ground their planning before producing code.

## End-user workflow

The intended non-coder workflow is:

```text
1. Start Veritas via Docker Compose.
2. CLI prints the Veritas logo and guided menu.
3. Configure services, vLLM models, chunking, ontology, and embeddings by prompt or config.
4. Ingest arXiv PDFs or upload local PDFs.
5. Parse PDFs with Docling.
6. Extract formulas and preserve formula context.
7. Chunk text without splitting formulas.
8. Embed chunks with normalized SBERT vectors.
9. Index text + formulas + vectors into OpenSearch FAISS/HNSW.
10. Map papers/chunks/formulas into Jena/Fuseki RDF.
11. Run SPARQL over the ontology graph to ground planning.
12. Retrieve evidence before making claims.
13. Perform representation-first math analysis.
14. Turn research/math into review-gated production-code packages.
15. Validate risks, assumptions, tests, control flow, and deployment constraints.
16. Return either results or meaningful failure messages with remediation.
```

## Model serving

vLLM is the model serving solution. Rust does **not** download and serve models
itself. The Rust API and CLI call vLLM's OpenAI-compatible HTTP endpoints, and
vLLM downloads Hugging Face models into the shared `hf-cache` Docker volume.

Default model routing:

| Role | Default | Alternatives |
|---|---|---|
| Planner | `Qwen/Qwen2.5-Coder-7B-Instruct` | any Hugging Face chat/code model served by vLLM |
| Code generation | `Qwen/Qwen2.5-Coder-14B-Instruct` | `Qwen/Qwen2.5-Coder-7B-Instruct`, `deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct` |
| Math reasoning | `allenai/Olmo-3-7B-Instruct` | `allenai/Olmo-3.1-32B-Instruct`, remote stronger model |
| Embeddings | `Muennighoff/SBERT-base-nli-v2` | any compatible SentenceTransformers model with updated dimension config |
| Ontology reasoning | Jena Fuseki + Openllet | external reasoner/validator can be wired later |

Configure interactively:

```bash
docker compose run --rm cli init
```

or edit:

```text
.env
config/veritas.yaml
```

Start model services:

```bash
# planner only
docker compose --profile models up -d vllm-planner

# code model
docker compose --profile code-model up -d vllm-code

# math model
docker compose --profile math-model up -d vllm-math

# all local vLLM roles
docker compose --profile models --profile code-model --profile math-model up -d
```

The vLLM containers require a CUDA-capable GPU and NVIDIA Container Toolkit. The
32B math model and 14B code model need substantially more VRAM than a small 6GB
GPU. Use the 7B defaults or a remote OpenAI-compatible endpoint when hardware is
limited.

## CLI startup experience

Running `veritas` or `docker compose run --rm cli welcome` opens a guided startup
screen instead of only printing command examples. The screen shows service health,
knowledge-graph counts, ontology/reasoner/vector-memory status, model routing,
workflow choices, and mode guidance.

```text
═══════════════════════════════════════════════════════════════════

██╗   ██╗███████╗██████╗ ██╗████████╗ █████╗ ███████╗
██║   ██║██╔════╝██╔══██╗██║╚══██╔══╝██╔══██╗██╔════╝
██║   ██║█████╗  ██████╔╝██║   ██║   ███████║███████╗
╚██╗ ██╔╝██╔══╝  ██╔══██╗██║   ██║   ██╔══██║╚════██║
 ╚████╔╝ ███████╗██║  ██║██║   ██║   ██║  ██║███████║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚══════╝

                 Mathematical Truth Through Evidence

      Math-heavy evidence-backed research and development
                software engineering agent

═══════════════════════════════════════════════════════════════════

System Status
─────────────
✓ OpenSearch FAISS/HNSW
✓ Jena Fuseki Graph
✓ Openllet Reasoner
✓ OWL-DL Ontology Loaded
✓ Embedding Service Ready
✓ Retrieval Pipeline Ready
! vLLM Planner Model
! vLLM Code Model
! vLLM Math Model

Knowledge Graph Status
──────────────────────
Objectives:                  27
Plans:                       83
Tasks:                      412
Risks:                       19
Invariants:                 153
Evidence Items:            1847
Validation Checks:           96

Model Routing
─────────────
Serving:   vLLM OpenAI-compatible endpoints
Planner:   Qwen/Qwen2.5-Coder-7B-Instruct
Code:      Qwen/Qwen2.5-Coder-14B-Instruct
Math:      allenai/Olmo-3-7B-Instruct
Embedding: Muennighoff/SBERT-base-nli-v2

What would you like to do?

[1] Ingest arXiv Research
[2] Upload Local PDFs
[3] Upload / Update Ontology
[4] Search Research Corpus
[5] Generate Code from Research
[6] Run Mathematical Discovery Workflow
[7] View Evidence Graph
[8] Validate Generated Artifacts
[9] Configuration

veritas >
```

## Key technologies

- **Docker Compose** for one-command local deployment.
- **vLLM** for OpenAI-compatible local model serving.
- **OpenSearch 3.7.0** with FAISS/HNSW `knn_vector` fields.
- **SentenceTransformers 5.5.1** using `Muennighoff/SBERT-base-nli-v2`.
- **Normalized embeddings** for cosine similarity.
- **Apache Jena Fuseki** for RDF/SPARQL graph storage.
- **Openllet** for offline ontology reasoning.
- **Veritas OWL-DL ontology** for cross-domain reasoning constraints.
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
  bootstrap.sh             Automated local startup
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

docker compose run --rm cli init
docker compose run --rm cli welcome
docker compose run --rm cli models
docker compose run --rm cli ingest-arxiv --query "cat:cs.AI OR cat:math.OC" --max-results 3
docker compose run --rm cli search "invariant representation" --mode hybrid
docker compose run --rm cli ask "turn indexed research into tested Rust code"
docker compose run --rm cli generate-code \
  --language rust \
  --prompt "implement the strongest indexed method as a tested package"
```

## OpenSearch FAISS/HNSW vector RAG

Veritas creates an OpenSearch index with:

```text
index.knn = true
field = embedding
type = knn_vector
engine = faiss
method = hnsw
space_type = cosinesimil
dimension = 768
```

Chunk embedding text includes:

```text
paper title
paper summary
chunk text
formula LaTeX
```

The embedding service normalizes vectors before indexing and querying so cosine
similarity behaves correctly.

## Ontology reasoning

Upload the ontology:

```bash
./scripts/upload-ontology.sh
```

Run SPARQL:

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

The ontology gives Veritas cross-domain constraints over concepts like:

```text
Objective
Plan
TaskSpecification
Risk
Invariant
EvidenceArtifact
ValidationCheckSpecification
SymbolicShadow
SourceCodeArtifact
BuildArtifact
```

## Failure messages

Veritas is expected to fail loudly and usefully. Failures should include:

```json
{
  "ok": false,
  "error": {
    "code": "plan.no_evidence",
    "message": "No OpenSearch evidence hits were found for the prompt.",
    "remediation": "Ingest arXiv papers or local PDFs first, then retry."
  }
}
```

## Current status

This scaffold supports ingestion, indexing, RDF graph mapping, SPARQL grounding,
vLLM model routing, evidence-backed planning drafts, and review-gated code package
generation. It still requires live Docker/GPU validation on your machine before
claiming production readiness for a specific deployment.
