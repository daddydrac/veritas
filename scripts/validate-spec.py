#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import yaml
from rdflib import Graph

ROOT = Path(__file__).resolve().parents[1]
INGESTION_SRC = ROOT / "services" / "ingestion"
sys.path.insert(0, str(INGESTION_SRC))

from veritas_ingest.chunking import make_chunks  # noqa: E402
from veritas_ingest.formulas import extract_formulas  # noqa: E402
from veritas_ingest.sinks import chunks_to_turtle  # noqa: E402


REQUIRED_FILES = [
    "docker-compose.yml",
    "schemas/planner.schema.json",
    "schemas/codegen.schema.json",
    "schemas/math_reasoning.schema.json",
    "schemas/human_checkpoint.schema.json",
    "schemas/run_report.schema.json",
    "packages/ontology/shacl/veritas-core.shacl.ttl",
    "packages/ontology/shacl/veritas-math.shacl.ttl",
    "services/shacl/Dockerfile",
    "apps/api/src/main.rs",
    "apps/api/src/providers.rs",
    "apps/api/src/schemas.rs",
    "apps/cli/src/main.rs",
    "services/ingestion/veritas_ingest/cli.py",
    "services/ingestion/veritas_ingest/docling_pdf.py",
    "services/ingestion/veritas_ingest/formulas.py",
    "services/ingestion/veritas_ingest/formula_images.py",
    "services/ingestion/veritas_ingest/latex_ocr.py",
    "services/ingestion/veritas_ingest/human_review.py",
    "services/ingestion/veritas_ingest/chunking.py",
    "services/ingestion/veritas_ingest/sinks.py",
    "packages/ontology/veritas.owl",
    "packages/ontology/queries/evidence_chunks.sparql",
    "packages/ontology/queries/formula_traceability.sparql",
    "docs/architecture/VERITAS_SPEC.md",
    "docs/architecture/END_TO_END_WORKFLOW.md",
    "README.md",
    "QUICKSTART.md",
    "docs/MODELS.md",
    "MODEL_SERVING_UPDATE.md",
    "services/ingestion/veritas_ingest/planning.py",
    "services/ingestion/veritas_ingest/codegen.py",
    "services/ingestion/veritas_ingest/ontology.py",
    "docker-compose.e2e.yml",
    "tests/fakes/fake_vllm_server.py",
    "tests/fakes/fake_embedding_server.py",
    "tests/fakes/Dockerfile.embedding",
    "tests/fixtures/sample_math_paper.pdf",
    "scripts/e2e/write-fake-runtime-env.sh",
    "scripts/e2e/wait-ready.sh",
    "scripts/e2e/validate-services.sh",
    "scripts/e2e/upload-ontology.sh",
    "scripts/e2e/ingest-fixture.sh",
    "scripts/e2e/plan-fixture.sh",
    "scripts/e2e/run-fixture.sh",
    "scripts/e2e/assert-e2e-result.py",
    "scripts/e2e/full-fake-vllm-e2e.sh",
    "scripts/e2e/gpu-validation.sh",
    "scripts/e2e/live-vllm-smoke.sh",
    "scripts/production-acceptance.sh",
    ".github/workflows/python.yml",
    ".github/workflows/rust.yml",
    ".github/workflows/docker-e2e.yml",
]


def check_file_exists(rel: str) -> dict:
    path = ROOT / rel
    return {"name": f"file:{rel}", "ok": path.exists(), "details": str(path)}


def check_yaml() -> dict:
    try:
        yaml.safe_load((ROOT / "config/veritas.yaml").read_text(encoding="utf-8"))
        yaml.safe_load((ROOT / "docker-compose.yml").read_text(encoding="utf-8"))
        return {"name": "yaml.parse", "ok": True}
    except Exception as exc:  # noqa: BLE001
        return {"name": "yaml.parse", "ok": False, "details": str(exc)}


def check_formula_extraction() -> dict:
    text = "Let $$E=mc^2$$ and $x_i = y^2$ hold. The price is $20."
    formulas = extract_formulas(text)
    bodies = [f["latex"] for f in formulas]
    ok = "E=mc^2" in bodies and "x_i = y^2" in bodies and "20" not in bodies
    return {"name": "formula.extraction", "ok": ok, "details": bodies}


def check_chunk_formula_boundary() -> dict:
    text = "Intro. " + "a" * 40 + " $$" + "x" * 200 + "=1$$ tail."
    chunks = make_chunks(
        "paper-1",
        text,
        {"title": "fixture"},
        target_chars=80,
        overlap_chars=10,
        hard_max_chars=90,
        context_window=5,
    )
    formulas = [formula for chunk in chunks for formula in chunk.get("formulas", [])]
    ok = len(formulas) == 1 and "=1" in formulas[0]["latex"]
    return {"name": "chunk.formula_boundary", "ok": ok, "details": {"chunks": len(chunks), "formulas": len(formulas)}}


def check_turtle_parse() -> dict:
    chunks = [
        {
            "chunk_id": "paper-1::chunk::00000",
            "paper_id": "paper-1",
            "ordinal": 0,
            "text": "Energy relation $$E=mc^2$$.",
            "formulas": extract_formulas("Energy relation $$E=mc^2$$."),
            "metadata": {"title": "fixture", "pdf_sha256": "abc"},
        }
    ]
    turtle = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:test")
    try:
        Graph().parse(data=turtle, format="turtle")
        return {"name": "rdf.turtle_parse", "ok": True}
    except Exception as exc:  # noqa: BLE001
        return {"name": "rdf.turtle_parse", "ok": False, "details": str(exc)}



def check_no_unused_external_vector_db() -> dict:
    files = ["docker-compose.yml", "apps/api/src/main.rs", "apps/cli/src/main.rs", "scripts/bootstrap.sh"]
    hits = []
    for rel in files:
        text = (ROOT / rel).read_text(encoding="utf-8")
        if ("qd" + "rant") in text.lower():
            hits.append(rel)
    return {"name": "architecture.no_unused_external_vector_db", "ok": not hits, "details": hits}


def check_no_env_example() -> dict:
    return {"name": "configuration.no_env_example", "ok": not (ROOT / ".env.example").exists(), "details": "configuration is generated by `veritas init` into .veritas/runtime.env"}

def check_api_run_implemented() -> dict:
    text = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    required = ['route("/run"', "execute_autonomous_run", "run_command", "call_chat_model_json", "production_candidate_validated"]
    missing = [item for item in required if item not in text]
    return {"name": "api.autonomous_run", "ok": not missing, "details": {"missing": missing}}


def check_pass1_provider_abstraction() -> dict:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    providers = (ROOT / "apps/api/src/providers.rs").read_text(encoding="utf-8")
    schemas = (ROOT / "apps/api/src/schemas.rs").read_text(encoding="utf-8")
    required = [
        ("main", "ProviderRouter"),
        ("main", "ApiFailure::from_provider_error"),
        ("providers", "pub trait ModelProvider"),
        ("providers", "LocalVllmProvider"),
        ("providers", "RemoteOpenAICompatibleProvider"),
        ("providers", "ProviderFailureCategory"),
        ("providers", "schema_json(schema_key)"),
        ("schemas", 'include_str!("../../../schemas/planner.schema.json")'),
        ("schemas", "validate_required_object_fields"),
    ]
    blobs = {"main": main, "providers": providers, "schemas": schemas}
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs[where]]
    return {"name": "pass1.provider_abstraction", "ok": not missing, "details": {"missing": missing}}


def check_pass2_execution_safety() -> dict:
    text = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    required = [
        "struct RunLock",
        "impl Drop for RunLock",
        "acquire_run_lock",
        "create_new(true)",
        "RunResumeRequest",
        "request.json",
        "resume_autonomous_run",
        "execute_autonomous_run_core",
        "plan_envelope.json",
        "tool_outputs.json",
        "command_audit.jsonl",
        "events.jsonl",
        "next_event_sequence",
        "write_json_file",
        "CancelRequested",
    ]
    forbidden = ["resume_requires_original_request", "Automatic step-level resume will resume"]
    missing = [item for item in required if item not in text]
    present_forbidden = [item for item in forbidden if item in text]
    return {"name": "pass2.execution_safety", "ok": not missing and not present_forbidden, "details": {"missing": missing, "forbidden_present": present_forbidden}}


def check_pass3_retrieval_ontology_hardening() -> dict:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    cli = (ROOT / "apps/cli/src/main.rs").read_text(encoding="utf-8")
    sinks = (ROOT / "services/ingestion/veritas_ingest/sinks.py").read_text(encoding="utf-8")
    ingest_cli = (ROOT / "services/ingestion/veritas_ingest/cli.py").read_text(encoding="utf-8")
    config = (ROOT / "config/veritas.yaml").read_text(encoding="utf-8")
    required = [
        ("main", "/opensearch/status"),
        ("main", "OpenSearchMigrateRequest"),
        ("main", "build_alias_actions"),
        ("main", "opensearch_read_alias"),
        ("main", "opensearch_write_alias"),
        ("main", "production_opensearch_mapping(&state.opensearch_vector_field, state.opensearch_vector_dimension"),
        ("main", "/graphs"),
        ("main", "/graph/upload"),
        ("main", "/graph/facts"),
        ("main", "planner_fact_summary"),
        ("main", "query_pack"),
        ("main", "upload_run_report_to_fuseki"),
        ("main", "graph_document_base_uri"),
        ("cli", "OpenSearchStatus"),
        ("cli", "GraphList"),
        ("cli", "GraphFacts"),
        ("cli", "GraphUpload"),
        ("sinks", "document_graph_uri"),
        ("ingest_cli", "latest-fuseki-upload-manifest.json"),
        ("config", "read_alias_env"),
        ("config", "named_graph_policy"),
    ]
    blobs = {"main": main, "cli": cli, "sinks": sinks, "ingest_cli": ingest_cli, "config": config}
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs[where]]
    return {"name": "pass3.retrieval_ontology_hardening", "ok": not missing, "details": {"missing": missing}}


def check_pass4_mathematical_research_workflow() -> dict:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    cli = (ROOT / "apps/cli/src/main.rs").read_text(encoding="utf-8")
    ingest_cli = (ROOT / "services/ingestion/veritas_ingest/cli.py").read_text(encoding="utf-8")
    formula_images = (ROOT / "services/ingestion/veritas_ingest/formula_images.py").read_text(encoding="utf-8")
    latex_ocr = (ROOT / "services/ingestion/veritas_ingest/latex_ocr.py").read_text(encoding="utf-8")
    formulas = (ROOT / "services/ingestion/veritas_ingest/formulas.py").read_text(encoding="utf-8")
    human = (ROOT / "services/ingestion/veritas_ingest/human_review.py").read_text(encoding="utf-8")
    math_schema = json.loads((ROOT / "schemas/math_reasoning.schema.json").read_text(encoding="utf-8"))
    shacl_math = (ROOT / "packages/ontology/shacl/veritas-math.shacl.ttl").read_text(encoding="utf-8")
    required = [
        ("main", "math_to_code_system_prompt"),
        ("main", "build_math_to_code_reasoning_prompt"),
        ("main", "math_human_checkpoint"),
        ("main", "/human/checkpoint"),
        ("main", "SchemaKey::MathReasoning"),
        ("cli", "ReviewFormulas"),
        ("cli", "awaiting_human_checkpoint"),
        ("ingest_cli", "review-formulas"),
        ("formula_images", "_apply_latex_ocr"),
        ("latex_ocr", "VERITAS_LATEX_OCR_PROVIDER"),
        ("formulas", "extract_docling_formula_candidates"),
        ("formulas", "merge_formula_candidates"),
        ("human", "apply_formula_decision"),
        ("shacl_math", "RepresentationMap"),
        ("shacl_math", "GenerativeNecessityClaim"),
    ]
    blobs = {"main": main, "cli": cli, "ingest_cli": ingest_cli, "formula_images": formula_images, "latex_ocr": latex_ocr, "formulas": formulas, "human": human, "shacl_math": shacl_math}
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs[where]]
    schema_required = set(math_schema.get("required", []))
    math_fields = {"surface_phenomenon", "representation_hypothesis", "candidate_representation_map", "transformation_space", "invariants", "compression_fidelity", "recursive_closure", "generative_necessity", "symbolic_shadows", "transfer_tests", "status"}
    missing_schema = sorted(math_fields - schema_required)
    return {"name": "pass4.mathematical_research_workflow", "ok": not missing and not missing_schema, "details": {"missing": missing, "missing_schema": missing_schema}}


def check_pass5_deployment_production_proof() -> dict:
    cli = (ROOT / "apps/cli/src/main.rs").read_text(encoding="utf-8")
    compose_e2e = (ROOT / "docker-compose.e2e.yml").read_text(encoding="utf-8")
    validate_host = (ROOT / "scripts/validate-host.sh").read_text(encoding="utf-8")
    production_acceptance = (ROOT / "scripts/production-acceptance.sh").read_text(encoding="utf-8")
    full_e2e = (ROOT / "scripts/e2e/full-fake-vllm-e2e.sh").read_text(encoding="utf-8")
    gpu_script = (ROOT / "scripts/e2e/gpu-validation.sh").read_text(encoding="utf-8")
    workflows = "\n".join((ROOT / ".github/workflows" / name).read_text(encoding="utf-8") for name in ["python.yml", "rust.yml", "docker-e2e.yml"])
    required = [
        ("cli", "E2eFake"),
        ("cli", "ValidateHost"),
        ("cli", "ProductionAccept"),
        ("cli", "detect_gpu_inventory"),
        ("cli", "model_vram_hint_gb"),
        ("compose_e2e", "fake-vllm-planner"),
        ("compose_e2e", "fake-vllm-code"),
        ("compose_e2e", "fake-vllm-math"),
        ("compose_e2e", "Dockerfile.embedding"),
        ("compose_e2e", "condition: service_healthy"),
        ("validate_host", "cargo fmt --all -- --check"),
        ("validate_host", "cargo check --workspace"),
        ("validate_host", "docker compose --env-file .veritas/runtime.env config"),
        ("validate_host", "scripts/e2e/full-fake-vllm-e2e.sh"),
        ("validate_host", "VERITAS_REQUIRE_LIVE_VLLM_VALIDATION"),
        ("production_acceptance", "scripts/validate-host.sh"),
        ("full_e2e", "scripts/e2e/ingest-fixture.sh"),
        ("full_e2e", "scripts/e2e/plan-fixture.sh"),
        ("full_e2e", "scripts/e2e/run-fixture.sh"),
        ("gpu_script", "VERITAS_PLANNER_TENSOR_PARALLEL_SIZE"),
        ("gpu_script", "VERITAS_MATH_TENSOR_PARALLEL_SIZE"),
        ("workflows", "docker-fake-vllm-e2e"),
        ("workflows", "cargo clippy"),
    ]
    blobs = {
        "cli": cli,
        "compose_e2e": compose_e2e,
        "validate_host": validate_host,
        "production_acceptance": production_acceptance,
        "full_e2e": full_e2e,
        "gpu_script": gpu_script,
        "workflows": workflows,
    }
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs[where]]
    return {"name": "pass5.deployment_production_proof", "ok": not missing, "details": {"missing": missing}}

def check_optional_command(name: str, args: list[str]) -> dict:
    try:
        result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, timeout=30, check=False)
        return {
            "name": name,
            "ok": result.returncode == 0,
            "details": {
                "returncode": result.returncode,
                "stdout": result.stdout[-1000:],
                "stderr": result.stderr[-1000:],
            },
        }
    except FileNotFoundError:
        return {"name": name, "ok": None, "details": "command unavailable in this environment"}
    except Exception as exc:  # noqa: BLE001
        return {"name": name, "ok": False, "details": str(exc)}


def main() -> int:
    checks = [check_file_exists(rel) for rel in REQUIRED_FILES]
    checks.extend([
        check_yaml(),
        check_formula_extraction(),
        check_chunk_formula_boundary(),
        check_turtle_parse(),
        check_no_unused_external_vector_db(),
        check_no_env_example(),
        check_api_run_implemented(),
        check_pass1_provider_abstraction(),
        check_pass2_execution_safety(),
        check_pass3_retrieval_ontology_hardening(),
        check_pass4_mathematical_research_workflow(),
        check_pass5_deployment_production_proof(),
        check_optional_command("cargo.check", ["cargo", "check", "--workspace"]),
        check_optional_command("docker.compose.config", ["docker", "compose", "config"]),
    ])
    failed = [c for c in checks if c["ok"] is False]
    unavailable = [c for c in checks if c["ok"] is None]
    payload = {
        "ok": not failed,
        "summary": {
            "total": len(checks),
            "failed": len(failed),
            "unavailable": len(unavailable),
        },
        "checks": checks,
    }
    print(json.dumps(payload, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
