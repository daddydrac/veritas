# Veritas Quickstart

This quickstart has two runnable paths:

1. **Source/mocked acceptance** — runs without Cargo, Docker, or live vLLM. Use this in this sandbox or any Python-only environment to verify the control-plane contracts, scorecard, documentation, and mocked E2E harness.
2. **Docker/live acceptance** — runs on your workstation or server with Docker, and optionally GPUs/vLLM.

The current scoped acceptance intentionally skips only these live-host dimensions when the `source-mocked` profile is used: Rust/Cargo validation, Docker E2E execution, and live vLLM/GPU validation. Everything else remains source/mocked tested.

---

## 1. Prerequisites

For source/mocked acceptance:

```bash
python3 --version
python3 -m pip install --upgrade pip
python3 -m pip install pytest pyyaml rdflib jsonschema
```

For Docker/live acceptance, also install:

```text
Docker
Docker Compose v2
curl
jq
NVIDIA Container Toolkit, if running local GPU vLLM
```

GPU smoke check for live vLLM profiles:

```bash
docker run --rm --gpus all nvidia/cuda:12.8.0-base-ubuntu24.04 nvidia-smi
```

---

## 2. Source/mocked verification path

Run this first after unzipping the repo. It verifies packaging, Python services, source/mocked E2E harnesses, SHACL/math governance contracts, formula OCR/review contracts, human checkpoint contracts, validation spec, and generated feature scorecard.

```bash
scripts/check-packaging.sh
python3 -m compileall services/embedding services/ingestion services/shacl tests/fakes scripts/e2e
PYTHONPATH=services/ingestion PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 pytest -q tests/ingestion --disable-warnings
python3 scripts/validate-spec.py
scripts/production-acceptance.sh --profile source-mocked
scripts/e2e/source-mocked-scorecard.sh
```

Expected result:

```text
validate-spec: ok=true, failed=0
production acceptance: mocked_acceptance
scorecard: source_mocked_ready
```

Generated scorecard outputs:

```text
data/scorecard/feature-scorecard.json
FEATURE_SCORECARD.md
```

---

## 3. Configure Veritas for Docker/live use

Do **not** copy or hand-maintain legacy environment template files; Veritas is configured through the CLI and generated `.veritas` files.

Interactive setup:

```bash
docker compose run --rm cli init
```

The wizard asks for:

```text
Planner model        default Qwen/Qwen2.5-Coder-7B-Instruct
Code model           default Qwen/Qwen2.5-Coder-14B-Instruct
Code fallback        default deepseek-ai/DeepSeek-Coder-V2-Lite-Instruct
Math model           default OLMo/OLMo-style instruct model or custom HF ID
Embedding model      default Muennighoff/SBERT-base-nli-v2
Hugging Face token   optional
GPU layout           role-specific IDs, tensor parallel, pipeline parallel, memory utilization
OpenSearch URL       default http://opensearch:9200 inside Compose
Fuseki URL           default http://fuseki:3030 inside Compose
SHACL URL            default http://shacl:8090 inside Compose
Human review policy  auto_approve, require_all, or require_high_risk_only
```

Generated files:

```text
.veritas/config.yaml
.veritas/runtime.env
.veritas/docker-compose.override.yaml
```

---

## 4. Start core services

For the normal service stack:

```bash
./scripts/bootstrap.sh
```

This starts:

```text
OpenSearch
OpenSearch Dashboards
Jena Fuseki
SHACL validator
SBERT embedding service
Veritas API
Veritas CLI service
```

Check readiness:

```bash
curl -s http://localhost:8080/ready | jq
curl -s http://localhost:8080/health | jq
```

---

## 5. Start vLLM model serving

Planner only:

```bash
docker compose --profile models up -d vllm-planner
```

Code writer:

```bash
docker compose --profile code-model up -d vllm-code
```

Math reasoner:

```bash
docker compose --profile math-model up -d vllm-math
```

All local model roles:

```bash
docker compose --profile models --profile code-model --profile math-model up -d
```

Show model routing and provider health:

```bash
docker compose run --rm cli models
curl -s http://localhost:8080/models | jq
```

For live GPU production acceptance:

```bash
scripts/production-acceptance.sh --profile single-gpu-prod
# or
scripts/production-acceptance.sh --profile multi-gpu-prod
```

---

## 6. Open the guided CLI startup screen

```bash
docker compose run --rm cli welcome
```

The startup screen prints the Veritas logo, tagline, service readiness, knowledge-graph status, model routing, workflow menu, and mode guidance.

---

## 7. Upload or refresh the ontology

Upload the bundled ontology:

```bash
docker compose run --rm cli upload-ontology
```

Upload a custom OWL/RDF/Turtle file:

```bash
docker compose run --rm cli upload-ontology --path ./my-ontology.owl
```

Fuseki stores ontology and project facts in named graphs. PDFs themselves are not uploaded into Fuseki; semantic facts, citations, chunks, formulas, run facts, validation findings, and artifact links are uploaded.

---

## 8. Ingest research papers

arXiv ingestion:

```bash
docker compose run --rm cli ingest-arxiv \
  --query "cat:cs.AI OR cat:math.OC" \
  --max-results 3
```

Local PDF ingestion:

```bash
docker compose run --rm cli ingest-pdf --path ./paper.pdf
```

Pipeline:

```text
PDF/arXiv metadata
→ APA citation generation and review state
→ Docling-first parsing with fallback extraction
→ formula candidates, image metadata, OCR status, normalized LaTeX
→ 25-word prose chunks up to the nearest period/semicolon
→ formula-preserving chunks
→ normalized SBERT embeddings
→ OpenSearch FAISS/HNSW index
→ Fuseki RDF named-graph upload
→ SHACL core + math governance checks
```

Review extracted formulas:

```bash
docker compose run --rm cli review-formulas --chunks data/chunks/latest.chunks.jsonl
```

Review citations:

```bash
docker compose run --rm cli review-citations --chunks data/chunks/latest.chunks.jsonl
```

Validate formula extraction quality:

```bash
docker compose run --rm cli validate-formulas --chunks data/chunks/latest.chunks.jsonl
```

---

## 9. Search evidence

Hybrid vector + lexical + formula search:

```bash
docker compose run --rm cli search \
  "representation learning invariant structure" \
  --mode hybrid \
  --size 5
```

Formula search:

```bash
docker compose run --rm cli search "E = mc^2" --mode lexical --size 5
```

---

## 10. Query the ontology graph

Formula traceability:

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

List graphs:

```bash
docker compose run --rm cli graph-list
```

Describe a graph:

```bash
docker compose run --rm cli graph-describe urn:veritas:graph:ontology
```

---

## 11. Ask Veritas to plan from research evidence

```bash
docker compose run --rm cli ask \
  "Use the indexed papers to design a tested Rust implementation of the main method."
```

The planner retrieves OpenSearch evidence, queries Fuseki/Jena for formula and project facts, runs SHACL gating where required, and calls the configured planner provider. Model outputs must satisfy structured JSON schemas before execution.

---

## 12. Run autonomous code generation and validation

```bash
docker compose run --rm cli run \
  "Implement the strongest indexed method as a tested package with CPU-safe implementation and GPU extension points." \
  --language rust
```

Alias:

```bash
docker compose run --rm cli generate-code \
  --language rust \
  --prompt "Implement the strongest indexed method as a tested package with CPU-safe implementation and GPU extension points."
```

The run creates a workspace under:

```text
data/runs/run-*/
```

Important files:

```text
request.json
state.json
events.jsonl
command_audit.jsonl
human_checkpoints.jsonl
final_report.json
automatic_shacl_report.json
source files
tests
build/test outputs
```

Generated package status changes to `production_candidate_validated` only after required validation commands pass and required human checkpoints pass or are explicitly waived.

---

## 13. Math-to-code workflow

Use a raw formula:

```bash
docker compose run --rm cli math-to-code \
  --formula-latex 'L(\\theta)=\\mathbb{E}_{q_\\theta(z)}[\\log p(x,z)-\\log q_\\theta(z)]' \
  --language rust
```

Use an extracted formula ID:

```bash
docker compose run --rm cli math-to-code \
  --formula-id urn:veritas:formula:paper:page_4:eq_2 \
  --language rust
```

The math workflow treats formulas as **SymbolicShadow** artifacts, not truth by themselves. It asks for surface phenomenon, representation map, latent ontology, transformation space, invariants, compression fidelity, recursion, generative necessity, transfer/proof status, and validation requirements before code generation.

---

## 14. Human checkpoint workflow

Review a single checkpoint:

```bash
docker compose run --rm cli review-checkpoint \
  --phase plan_review \
  --decision approve \
  --reviewer "human@example.com" \
  --artifact-id run-123-plan
```

Run the source/mocked human workflow proof:

```bash
scripts/e2e/source-mocked-human-workflow.sh
```

Supported phases:

```text
citation_review
formula_review
representation_review
plan_review
code_architecture_review
validation_review
```

---

## 15. Direct model call

```bash
docker compose run --rm cli chat \
  --role planner \
  "Summarize the current evidence-backed implementation plan."
```

Roles:

```text
planner
code
math
```

---

## 16. Logs and troubleshooting

```bash
docker compose logs -f api
docker compose logs -f embedding
docker compose logs -f ingestion
docker compose logs -f shacl
docker compose logs -f vllm-planner
docker compose logs -f opensearch
docker compose logs -f fuseki
```

Host validation summary:

```bash
cat data/e2e/host-validation-summary.json | jq
```

Feature scorecard:

```bash
cat data/scorecard/feature-scorecard.json | jq
cat FEATURE_SCORECARD.md
```

---

## 17. Stop services

```bash
docker compose down
```

Remove data volumes:

```bash
docker compose down -v
```


## Real Journey Orchestrator Quickstart

The canonical end-user path starts with the journey command:

```bash
veritas journey run --source tests/fixtures/sample_math_paper.pdf --mode local --goal "Turn this paper into validated software" --language rust
```

Inspect progress and reports with:

```bash
veritas journey status <run_id>
veritas journey report <run_id>
```

Record human checkpoint decisions with:

```bash
veritas journey review <run_id> --phase plan_review --decision approve --notes "Plan approved for implementation."
```

Phase 1 registers source documents and delegates to the existing real autonomous run core. Phase 2 adds real local ingestion so a PDF produces evidence manifests, citation manifests, formula manifests, and review queues before planning.

## Phase 2 Real Local Ingestion Backend

For a real local paper-to-evidence run without OpenSearch, Fuseki, Docker service DNS, or mocked proof scripts, run:

```bash
PYTHONPATH=services/ingestion \
python3 -m veritas_ingest.cli --config config/veritas.yaml ingest-pdf \
  --path tests/fixtures/sample_math_paper.pdf \
  --backend local \
  --workspace data/runs/local-ingestion-demo
```

This writes real local ingestion artifacts:

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

If no real local embedding provider is configured, ingestion still succeeds and the evidence manifest sets `planning_status=blocked_retrieval_unavailable`. That is intentional: Veritas does not fabricate embeddings or pretend a production-bound retrieval path is ready.

To enable real local embeddings, install sentence-transformers and configure the local provider, or start the embedding service and use the HTTP provider:

```bash
export VERITAS_LOCAL_EMBEDDING_PROVIDER=sentence-transformers
# or
export VERITAS_LOCAL_EMBEDDING_PROVIDER=http
export VERITAS_EMBEDDING_URL=http://localhost:8090
```

## Evidence Eligibility Registry check

After local ingestion, inspect the real evidence gate before planning or formula-to-code:

```bash
PYTHONPATH=services/ingestion python3 -m veritas_ingest.cli evidence-registry \
  --workspace data/runs/<run_id>/ingestion \
  --refresh-from-chunks
```

A production-bound journey requires approved citations and eligible formulas. If the registry reports `awaiting_evidence_review`, review citations/formulas first. Raw `human_approved=true` style overrides are not accepted as production authority.

## Pre-Execution Gate Engine

After ingestion and evidence review, Veritas must pass the Pre-Execution Gate Engine before code generation begins.

For governed runs, approve the required checkpoints before resuming:

```bash
veritas journey review <run_id> --phase plan_review --decision approve --notes "Plan reviewed."
veritas journey review <run_id> --phase code_architecture_review --decision approve --notes "Architecture approved."
veritas journey resume <run_id>
```

If approval is missing, Veritas stops before code generation and writes `pre_codegen_gate_report.json`. No generated files are written and no validation commands are run until the missing gate is resolved.

## Phase 5 — Tool-Verified Math Engine

Veritas now includes a real Tool-Verified Math Engine. Math-heavy runs no longer have to rely only on LLM reasoning before code generation. The application can call the `math-tools` service, persist `math_tool_calls.jsonl`, `math_tool_results.jsonl`, and `math_validation_report.json`, and the pre-codegen Gate Engine blocks when the report contains blocking findings or counterexamples.

The math-tools service exposes real executable tools: `parse_latex`, `normalize_expression`, `symbolic_simplify`, `symbolic_differentiate`, `symbolic_equivalence`, `numeric_validate`, `counterexample_search`, `dimension_check`, and `generate_property_tests`. The service uses SymPy, NumPy, SciPy/mpmath-compatible numeric evaluation, and generated property-test code. No model output is treated as mathematical truth unless tool results, governance gates, and validation artifacts support it.


### SHACL governance mode

Set `VERITAS_GOVERNANCE_MODE=enforce` for governed local or production journeys. Use `advisory` only for exploratory development. Use `disabled` only when explicitly accepting that the run cannot claim production validation.


## Phase 7 — Artifact Decision Engine

Phase 7 adds a canonical Artifact Decision Engine in `apps/api/src/artifact_decision.rs`. Final artifact status is no longer granted directly by the code-generation loop. The engine reads real run artifacts, gate decisions, validation results, human checkpoint state, SHACL results, and host-validation evidence before producing `artifact_decision.json`.

Important behavior:

- validation success alone does not imply production readiness;
- missing human approval results in `awaiting_human_approval`;
- failed SHACL results in `blocked_by_governance` when governance is enforced;
- failed validation results in `validation_failed` or `repair_failed`;
- missing host validation results in `local_validated_host_pending`;
- `production_validated` is only possible when host validation evidence exists and passes.

## Phase 8 lineage verification

When running a journey or `/run`, generated files now require explicit lineage. A code model response that omits `derived_from_plan_step_ids`, `derived_from_evidence_ids`, `derived_from_citation_ids`, `derived_from_formula_ids`, or `required_validation_ids` is rejected before any file is written.

To inspect lineage after a run, open:

```bash
cat data/runs/<run_id>/lineage_context.json
cat data/runs/<run_id>/planning_context.json
cat data/runs/<run_id>/file_lineage.json
cat data/runs/<run_id>/final_report.json
```

The final report now contains `plan_lineage`, `file_lineage`, `command_lineage`, `validation_lineage`, `repair_lineage`, and `governance_lineage`.


## Phase 8 lineage check

Veritas now rejects model-generated code before file writes unless the generated package includes explicit lineage back to approved planning, evidence, citation, formula, and validation identifiers. To verify the local lineage contract:

```bash
PYTHONPATH=services/ingestion PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 \
pytest -q tests/ingestion/test_phase8_lineage_schemas.py --disable-warnings
```

## Phase 9 quick check — approved evidence before planning

For production-bound use, run ingestion and review before planning. Veritas now writes `planning_context.json` and blocks planning if there are no approved citations or eligible evidence records.

```bash
PYTHONPATH=services/ingestion python3 -m veritas_ingest.cli --config config/veritas.yaml ingest-pdf \
  --path tests/fixtures/sample_math_paper.pdf \
  --backend local \
  --workspace data/runs/phase9-demo

PYTHONPATH=services/ingestion python3 -m veritas_ingest.cli review-citations \
  --chunks data/runs/phase9-demo/chunks.jsonl \
  --decision approve

PYTHONPATH=services/ingestion python3 -m veritas_ingest.cli review-formulas \
  --chunks data/runs/phase9-demo/chunks.jsonl \
  --decision approve
```

After review, the Evidence Eligibility Registry contains the approved citation and eligible formula IDs that planning must cite.

## Evidence-grounded planning behavior

For real production-bound journeys, Veritas now requires approved evidence before planning. After ingestion and evidence review, the journey writes `evidence_registry.json`. Planning then writes `planning_context.json` from approved evidence, approved citations, eligible formulas, ontology facts, and retrieval results.

If evidence has not been reviewed, planning stops with an actionable error instead of asking the planner model to rely on memory. For non-production exploration only, set `execution_mode=dev_exploratory` and `VERITAS_ALLOW_EMPTY_EVIDENCE=true`; the final artifact decision will remain `dev_only_unverified`.
