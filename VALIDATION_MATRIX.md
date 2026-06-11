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
