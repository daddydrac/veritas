# Veritas Production Validation Matrix

| Capability | Source implemented | Python tests | Rust tests | Docker/live validated | Notes |
|---|---:|---:|---:|---:|---|
| CLI setup writes `.veritas` config | yes | partial | not run | not run | Cargo unavailable here. |
| vLLM planner/code/math config | yes | n/a | not run | not run | Docker/GPU unavailable here. |
| OpenSearch FAISS/HNSW mapping | yes | yes | not run | not run | Mapping tested with fake client. |
| PDF chunking at 25 words + formula preservation | yes | yes | n/a | not run | Docling live validation still host-dependent. |
| APA citation metadata | yes | yes | n/a | not run | Human confirmation still CLI future work. |
| Fuseki RDF output | yes | yes | not run | not run | Turtle parse tested; live Fuseki unavailable. |
| SHACL rule pack/service | yes | rule presence | not run | not run | Live pySHACL service requires Docker. |
| `/plan` model-backed planning | yes | n/a | not run | not run | Requires cargo + vLLM host validation. |
| `/run` bounded code/test loop | yes | n/a | not run | not run | Requires cargo + vLLM host validation. |
| Legacy codegen quarantine | yes | yes | n/a | n/a | No false production status in Python path. |

| Role-specific structured-output routing | yes | source grep | not run | not run | Planner/codegen/math schemas are selected by role; live vLLM structured decoding still host-validated. |
| Pass 1 provider abstraction | yes | source grep | not run | not run | `providers.rs` defines `ModelProvider`, local vLLM, remote OpenAI-compatible fallback, provider router, and failure taxonomy; live Rust/vLLM validation still requires host tools. |
| Fake vLLM E2E profile | yes | fake server compiles | not run | not run | `docker-compose.e2e.yml` added; Docker unavailable here. |
| Math-to-code endpoint | yes | source grep | not run | not run | `/math-to-code` wraps run loop with formula-aware goal; deeper dedicated workflow remains next iteration. |
| Run status/resume/cancel | yes | source grep | not run | not run | Durable run state, locking, cancellation, and artifact-aware resume are source-implemented; live Rust/Docker validation still requires host tools. |
| Sandbox command runner | yes | source grep | not run | not run | Docker sandbox path added; requires host Docker validation. |
| Formula image metadata | yes | yes | n/a | not run | Optional PyMuPDF rasterization with explicit fallback statuses. |


| Pass 2 execution safety | yes | source grep | not run | not run | Durable run workspaces, atomic `run.lock`, persisted request/state/events/artifacts, resumable `/run/:run_id/resume`, cancellation marker, and command audit log added; live Rust/Docker validation still requires host tools. |

| Pass 3 retrieval and ontology hardening | yes | yes | not run | not run | Rust API now owns versioned OpenSearch mapping/migration with aliases, exposes graph upload/list/describe/facts endpoints, groups ingestion RDF by document named graph, and writes run/validation facts back to Fuseki; live OpenSearch/Fuseki validation still requires Docker. |
| OpenSearch versioned aliases | yes | source grep | not run | not run | `/opensearch/migrate` creates a versioned FAISS/HNSW index and read/write aliases; live alias mutation must be host-validated. |
| Fuseki named graph discipline | yes | Turtle parse | not run | not run | Ontology graph, document graph, run graph, and validation graph are explicit; ingestion uploads document ABox facts per document graph. |
| Planner SPARQL fact summary | yes | source grep | not run | not run | API query pack summarizes validation gaps, risks, formulas, invariants, plans, source artifacts, builds, loops, assumptions, deployment observability, and transfer tests. |

## Pass 4 — Mathematical research workflow

| Capability | Source-level status | Sandbox validation | Host validation |
|---|---:|---:|---:|
| Docling visual formula candidate extraction | implemented | Python tests passed | pending live PDF corpus |
| Formula image metadata and rasterization hook | implemented | Python tests passed | pending PyMuPDF + real PDFs |
| Pluggable LaTeX OCR | implemented | Python tests passed | pending configured OCR provider |
| Human formula review | implemented | Python tests passed | pending CLI/Docker host run |
| Representation-first math reasoning schema | implemented | schema parse + validator passed | pending live vLLM |
| Math-to-code human checkpoint | implemented | source validator passed | pending Rust/API host run |
| SHACL math rule pack | implemented | RDF/SHACL files present | pending live SHACL service |

## Pass 5 — Deployment and production proof

| Capability | Source-level status | Sandbox validation | Host validation |
|---|---:|---:|---:|
| Fake-vLLM planner/code/math E2E profile | implemented | file/script tests | pending Docker host |
| Fake embedding service for CI E2E | implemented | Python compile/tests | pending Docker host |
| Sample PDF fixture for ingestion E2E | implemented | file tests | pending Docker host |
| Full fake-vLLM E2E script | implemented | source tests | pending Docker host |
| OpenSearch migration proof in E2E | implemented | source tests | pending Docker host |
| Fuseki ontology upload proof in E2E | implemented | source tests | pending Docker host |
| `/plan` and `/run` E2E assertions | implemented | source tests | pending Docker host |
| GPU layout validation | implemented | source tests | pending NVIDIA host |
| Live vLLM smoke validation | implemented | source tests | pending GPU host |
| Host validation script | implemented | source tests | pending host run |
| Production acceptance script | implemented | source tests | pending host run |
| GitHub Actions CI definitions | implemented | source tests | pending remote CI |
