#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import os
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



def safe_read_text(rel: str, *, required: bool = False) -> tuple[str, str | None]:
    path = ROOT / rel
    try:
        return path.read_text(encoding="utf-8"), None
    except FileNotFoundError:
        message = f"missing file: {rel}"
        if required:
            return "", message
        return "", message
    except Exception as exc:  # noqa: BLE001
        return "", f"could not read {rel}: {exc}"


def check_contains_all(name: str, blobs: dict[str, str], required: list[tuple[str, str]], read_errors: list[str] | None = None) -> dict:
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    return {"name": name, "ok": not missing and not (read_errors or []), "details": {"missing": missing, "read_errors": read_errors or []}}

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
    "scripts/e2e/source-mocked-control-plane-e2e.py",
    "scripts/e2e/source-mocked-control-plane-e2e.sh",
    "scripts/e2e/source-mocked-execution-safety.py",
    "scripts/e2e/source-mocked-execution-safety.sh",
    "scripts/e2e/source-mocked-retrieval-ontology.py",
    "scripts/e2e/source-mocked-retrieval-ontology.sh",
    "services/ingestion/veritas_ingest/retrieval_ontology_contracts.py",
    "tests/ingestion/test_phase4_retrieval_ontology.py",
    "docs/tutorials/PHASE4_RETRIEVAL_ONTOLOGY_SOURCE.md",
    "docs/tutorials/PHASE5_SHACL_MATH_GOVERNANCE.md",
    "tests/ingestion/test_phase5_shacl_governance.py",
    "scripts/e2e/source-mocked-shacl-governance.sh",
    "scripts/e2e/source-mocked-shacl-governance.py",
    "services/ingestion/veritas_ingest/shacl_governance_contracts.py",
    "services/ingestion/veritas_ingest/formula_ocr_review_contracts.py",
    "tests/ingestion/test_phase6_formula_ocr_review.py",
    "scripts/e2e/source-mocked-formula-ocr-review.py",
    "scripts/e2e/source-mocked-formula-ocr-review.sh",
    "docs/tutorials/PHASE6_FORMULA_OCR_REVIEW.md",
    "scripts/e2e/wrap-final-report.py",
    "scripts/e2e/record-step.py",
    "scripts/e2e/gpu-validation.sh",
    "scripts/e2e/live-vllm-smoke.sh",
    "scripts/production-acceptance.sh",
    ".github/workflows/python.yml",
    ".github/workflows/rust.yml",
    ".github/workflows/docker-e2e.yml",
]


def check_file_exists(rel: str) -> dict:
    path = ROOT / rel
    exists = path.exists()
    details = {"path": str(path)}
    if not exists:
        details["remediation"] = "Restore the file or remove it from REQUIRED_FILES only if the feature is intentionally out of scope."
    return {"name": f"file:{rel}", "ok": exists, "details": details}


def read_text_optional(rel: str) -> tuple[str, bool]:
    path = ROOT / rel
    try:
        return path.read_text(encoding="utf-8"), True
    except FileNotFoundError:
        return "", False


def check_shell_scripts_executable() -> dict:
    scripts_dir = ROOT / "scripts"
    shell_scripts = sorted(scripts_dir.rglob("*.sh"))
    missing = [str(path.relative_to(ROOT)) for path in shell_scripts if not os.access(path, os.X_OK)]
    return {
        "name": "phase0.shell_scripts_executable",
        "ok": not missing,
        "details": {
            "checked": [str(path.relative_to(ROOT)) for path in shell_scripts],
            "not_executable": missing,
        },
    }


def check_validation_scope_markers() -> dict:
    matrix = (ROOT / "VALIDATION_MATRIX.md").read_text(encoding="utf-8")
    audit = (ROOT / "AUDIT.md").read_text(encoding="utf-8")
    required = [
        "source/mocked acceptance",
        "live host acceptance",
        "host_validation_pending",
        "cargo.check",
        "docker.compose.config",
        "live_vllm_smoke",
    ]
    missing = [needle for needle in required if needle not in matrix + audit]
    return {"name": "phase0.validation_scope_markers", "ok": not missing, "details": {"missing": missing}}


def check_production_acceptance_profiles() -> dict:
    script = (ROOT / "scripts/production-acceptance.sh").read_text(encoding="utf-8")
    required = [
        "fake-ci",
        "single-gpu-prod",
        "multi-gpu-prod",
        "remote-model-prod",
        "acceptance_mode",
        "live_gpu_acceptance",
        "mocked_acceptance",
    ]
    missing = [needle for needle in required if needle not in script]
    return {"name": "phase0.production_acceptance_profiles", "ok": not missing, "details": {"missing": missing}}


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
    fake_vllm = (ROOT / "tests/fakes/fake_vllm_server.py").read_text(encoding="utf-8")
    required = [
        ("main", "ProviderRouter"),
        ("main", "ApiFailure::from_provider_error"),
        ("main", "validate_json_schema"),
        ("main", "state.provider_router.health_for_role"),
        ("providers", "pub trait ModelProvider"),
        ("providers", "LocalVllmProvider"),
        ("providers", "RemoteOpenAICompatibleProvider"),
        ("providers", "ProviderFailureCategory"),
        ("providers", "ProviderFailureCategory::CircuitOpen"),
        ("providers", "ProviderRetryPolicy"),
        ("providers", "circuit_failure_threshold"),
        ("providers", "openai_compatible_models_health"),
        ("providers", "/v1/models"),
        ("providers", "VERITAS_REMOTE_PLANNER_MODEL"),
        ("providers", "VERITAS_REMOTE_CODE_MODEL"),
        ("providers", "VERITAS_REMOTE_MATH_MODEL"),
        ("providers", "extra_body"),
        ("providers", "guided_json"),
        ("schemas", 'include_str!("../../../schemas/planner.schema.json")'),
        ("schemas", "validate_json_schema"),
        ("schemas", "JSONSchema::options"),
        ("schemas", "Draft::Draft7"),
        ("fake_vllm", "FAKE_VLLM_RESPONSE_MODE"),
        ("fake_vllm", "unknown_tool"),
    ]
    blobs = {"main": main, "providers": providers, "schemas": schemas, "fake_vllm": fake_vllm}
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs[where]]
    return {"name": "pass1.provider_schema_hardening", "ok": not missing, "details": {"missing": missing}}


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
    files = {
        "cli": "apps/cli/src/main.rs",
        "compose_e2e": "docker-compose.e2e.yml",
        "validate_host": "scripts/validate-host.sh",
        "production_acceptance": "scripts/production-acceptance.sh",
        "full_e2e": "scripts/e2e/full-fake-vllm-e2e.sh",
        "gpu_script": "scripts/e2e/gpu-validation.sh",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    workflow_texts = []
    for name in ["python.yml", "rust.yml", "docker-e2e.yml"]:
        value, error = safe_read_text(f".github/workflows/{name}")
        workflow_texts.append(value)
        if error:
            read_errors.append(error)
    blobs["workflows"] = "\n".join(workflow_texts)
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
        ("validate_host", "VERITAS_SKIP_CARGO_VALIDATION"),
        ("validate_host", "VERITAS_SKIP_DOCKER_VALIDATION"),
        ("validate_host", "mocked_acceptance"),
        ("production_acceptance", "--profile"),
        ("production_acceptance", "source-mocked"),
        ("production_acceptance", "live_gpu_acceptance"),
        ("production_acceptance", "scripts/validate-host.sh --profile"),
        ("full_e2e", "scripts/e2e/ingest-fixture.sh"),
        ("full_e2e", "scripts/e2e/plan-fixture.sh"),
        ("full_e2e", "scripts/e2e/run-fixture.sh"),
        ("gpu_script", "VERITAS_PLANNER_TENSOR_PARALLEL_SIZE"),
        ("gpu_script", "VERITAS_MATH_TENSOR_PARALLEL_SIZE"),
        ("workflows", "docker-fake-vllm-e2e"),
        ("workflows", "cargo clippy"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    return {"name": "pass5.deployment_production_proof", "ok": not missing and not read_errors, "details": {"missing": missing, "read_errors": read_errors}}


def check_shell_scripts_executable() -> dict:
    scripts = sorted((ROOT / "scripts").rglob("*.sh"))
    missing_exec = [str(path.relative_to(ROOT)) for path in scripts if not (path.stat().st_mode & 0o111)]
    return {
        "name": "phase0.packaging.shell_scripts_executable",
        "ok": not missing_exec,
        "details": {
            "checked": len(scripts),
            "not_executable": missing_exec,
            "remediation": "Run chmod +x scripts/*.sh scripts/e2e/*.sh and package with scripts/package-release.sh.",
        },
    }


def check_phase0_acceptance_profiles() -> dict:
    production_acceptance, pa_error = safe_read_text("scripts/production-acceptance.sh")
    validate_host, vh_error = safe_read_text("scripts/validate-host.sh")
    matrix, matrix_error = safe_read_text("VALIDATION_MATRIX.md")
    required = [
        ("production_acceptance", "source-mocked"),
        ("production_acceptance", "fake-ci"),
        ("production_acceptance", "single-gpu-prod"),
        ("production_acceptance", "multi-gpu-prod"),
        ("production_acceptance", "live_gpu_acceptance"),
        ("validate_host", "VERITAS_SKIP_CARGO_VALIDATION"),
        ("validate_host", "VERITAS_SKIP_DOCKER_VALIDATION"),
        ("validate_host", "host-validation-steps.jsonl"),
        ("matrix", "Source/mocked acceptance"),
        ("matrix", "Live host acceptance"),
    ]
    blobs = {"production_acceptance": production_acceptance, "validate_host": validate_host, "matrix": matrix}
    errors = [e for e in [pa_error, vh_error, matrix_error] if e]
    return check_contains_all("phase0.acceptance_profiles", blobs, required, errors)


def check_phase2_source_mocked_e2e() -> dict:
    files = {
        "script_py": "scripts/e2e/source-mocked-control-plane-e2e.py",
        "script_sh": "scripts/e2e/source-mocked-control-plane-e2e.sh",
        "assertion": "scripts/e2e/assert-e2e-result.py",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("script_py", "planner_payload"),
        ("script_py", "math_payload"),
        ("script_py", "initial_codegen_payload"),
        ("script_py", "repaired_codegen_payload"),
        ("script_py", "validate(\"planner\""),
        ("script_py", "validate(\"codegen\""),
        ("script_py", "validate(\"math_reasoning\""),
        ("script_py", "validate(\"run_report\""),
        ("script_py", "command_audit.jsonl"),
        ("script_py", "final_report.json"),
        ("script_sh", "assert-e2e-result.py"),
        ("script_sh", "wrap-final-report.py"),
        ("validate_host", "Source-mocked control-plane E2E"),
        ("python_workflow", "Run Source-mocked control-plane E2E"),
        ("matrix", "Source-mocked control-plane E2E"),
        ("audit", "Phase 2"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-control-plane-e2e.py", "scripts/e2e/source-mocked-control-plane-e2e.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase2.source_mocked_control_plane_e2e", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}

def check_phase3_execution_safety() -> dict:
    files = {
        "main": "apps/api/src/main.rs",
        "script_py": "scripts/e2e/source-mocked-execution-safety.py",
        "script_sh": "scripts/e2e/source-mocked-execution-safety.sh",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("main", "effective_command_runner"),
        ("main", "VERITAS_ALLOW_LOCAL_COMMAND_RUNNER"),
        ("main", "local_blocked"),
        ("main", "command_rejection_reason"),
        ("main", "--pids-limit"),
        ("main", "--cap-drop"),
        ("main", "--read-only"),
        ("main", "--tmpfs"),
        ("main", "safe_output_path"),
        ("main", "validate_relative_output_path"),
        ("main", "reject_existing_symlink"),
        ("main", "verify_existing_path_inside_workspace"),
        ("main", "run_index.jsonl"),
        ("main", "command_audit_tail"),
        ("script_py", "symlink_parent_rejected"),
        ("script_py", "command_allowlist_rejects_dangerous"),
        ("script_py", "production_profile_defaults_to_sandbox"),
        ("script_py", "duplicate_lock_rejected"),
        ("script_py", "stale_lock_replaced"),
        ("script_py", "resume_cancelled_blocked"),
        ("script_sh", "source-mocked-execution-safety.py"),
        ("validate_host", "Source-mocked execution safety"),
        ("python_workflow", "Run Source-mocked execution safety"),
        ("matrix", "Phase 3"),
        ("audit", "Phase 3"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-execution-safety.py", "scripts/e2e/source-mocked-execution-safety.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase3.execution_safety", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}



def check_phase4_retrieval_ontology_source_mocked() -> dict:
    files = {
        "main": "apps/api/src/main.rs",
        "contracts": "services/ingestion/veritas_ingest/retrieval_ontology_contracts.py",
        "script_py": "scripts/e2e/source-mocked-retrieval-ontology.py",
        "script_sh": "scripts/e2e/source-mocked-retrieval-ontology.sh",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "schema": "schemas/opensearch/evidence_document.schema.json",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("main", "production_opensearch_mapping"),
        ("main", "build_alias_actions"),
        ("main", "retrieve_evidence"),
        ("main", "upload_turtle_to_fuseki"),
        ("main", "planner_fact_summary"),
        ("main", "query_pack"),
        ("main", "upload_run_report_to_fuseki"),
        ("contracts", "build_opensearch_mapping"),
        ("contracts", "mapping_contract_violations"),
        ("contracts", "MockOpenSearchTransport"),
        ("contracts", "retrieval_fallback"),
        ("contracts", "named_graph_uris"),
        ("contracts", "graph_store_request"),
        ("contracts", "contains_pdf_binary"),
        ("contracts", "run_report_to_turtle"),
        ("contracts", "summarize_sparql_results"),
        ("contracts", "DEFAULT_QUERY_PACK_NAMES"),
        ("script_py", "source_mocked_phase4_summary"),
        ("script_py", "phase4-summary.json"),
        ("script_sh", "source-mocked-retrieval-ontology.py"),
        ("validate_host", "Source-mocked retrieval ontology"),
        ("python_workflow", "Run Source-mocked retrieval ontology"),
        ("schema", "formula_embedding"),
        ("schema", "citations"),
        ("matrix", "Phase 4"),
        ("audit", "Phase 4"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-retrieval-ontology.py", "scripts/e2e/source-mocked-retrieval-ontology.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase4.source_mocked_retrieval_ontology", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}



def check_phase5_shacl_math_governance() -> dict:
    files = {
        "main": "apps/api/src/main.rs",
        "contracts": "services/ingestion/veritas_ingest/shacl_governance_contracts.py",
        "script_py": "scripts/e2e/source-mocked-shacl-governance.py",
        "script_sh": "scripts/e2e/source-mocked-shacl-governance.sh",
        "core": "packages/ontology/shacl/veritas-core.shacl.ttl",
        "math": "packages/ontology/shacl/veritas-math.shacl.ttl",
        "ontology": "packages/ontology/veritas.owl",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
        "features": "FEATURES.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("main", "veritas-math.shacl.ttl"),
        ("main", "collect_shacl_data_ttl"),
        ("main", "shacl_graph_context_construct_query"),
        ("main", "shacl_findings_to_turtle"),
        ("main", "automatic_shacl_findings.ttl"),
        ("contracts", "load_combined_shape_pack"),
        ("contracts", "validate_math_governance_contract"),
        ("contracts", "complete_math_to_code_ttl"),
        ("contracts", "invalid_validated_build_ttl"),
        ("contracts", "source_mocked_phase5_summary"),
        ("script_py", "source_mocked_phase5_summary"),
        ("script_py", "phase5-summary.json"),
        ("script_sh", "source-mocked-shacl-governance.py"),
        ("core", "ProductionBuildArtifactValidationShape"),
        ("core", "ShaclFindingShape"),
        ("math", "SymbolicShadowExtractionReadinessShape"),
        ("math", "MathToCodeValidationRequirementShape"),
        ("ontology", "hasValidationRequirement"),
        ("validate_host", "Source-mocked SHACL governance"),
        ("python_workflow", "Run Source-mocked SHACL governance"),
        ("matrix", "Phase 5"),
        ("audit", "Phase 5"),
        ("features", "Phase 5"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-shacl-governance.py", "scripts/e2e/source-mocked-shacl-governance.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase5.shacl_math_governance", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}


def check_phase6_formula_ocr_review() -> dict:
    files = {
        "formula_images": "services/ingestion/veritas_ingest/formula_images.py",
        "latex_ocr": "services/ingestion/veritas_ingest/latex_ocr.py",
        "human": "services/ingestion/veritas_ingest/human_review.py",
        "sinks": "services/ingestion/veritas_ingest/sinks.py",
        "cli": "services/ingestion/veritas_ingest/cli.py",
        "rust_cli": "apps/cli/src/main.rs",
        "contracts": "services/ingestion/veritas_ingest/formula_ocr_review_contracts.py",
        "script_py": "scripts/e2e/source-mocked-formula-ocr-review.py",
        "script_sh": "scripts/e2e/source-mocked-formula-ocr-review.sh",
        "ontology": "packages/ontology/veritas.owl",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
        "features": "FEATURES.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("formula_images", "VERITAS_FORMULA_IMAGE_RENDERER"),
        ("formula_images", "rendered_mock"),
        ("formula_images", "formula_image_engine"),
        ("formula_images", "bbox_status"),
        ("latex_ocr", "VERITAS_LATEX_OCR_COMMAND"),
        ("latex_ocr", "VERITAS_LATEX_OCR_URL"),
        ("latex_ocr", "_parse_latex_payload"),
        ("human", "apply_citation_decision"),
        ("human", "review_citations_in_chunks"),
        ("human", "codegen_eligibility_status"),
        ("sinks", "citation_review_status"),
        ("sinks", "formula_image_engine"),
        ("sinks", "hasCodegenEligibilityStatus"),
        ("cli", "review-citations"),
        ("cli", "validate-formulas"),
        ("rust_cli", "ReviewCitations"),
        ("rust_cli", "ValidateFormulas"),
        ("contracts", "command_ocr_contract"),
        ("contracts", "http_ocr_contract"),
        ("contracts", "formula_image_contract"),
        ("contracts", "review_contract"),
        ("contracts", "chunking_edge_contract"),
        ("contracts", "opensearch_mapping_contract"),
        ("contracts", "source_mocked_phase6_summary"),
        ("script_py", "source_mocked_phase6_summary"),
        ("script_py", "phase6-summary.json"),
        ("script_sh", "source-mocked-formula-ocr-review.py"),
        ("ontology", "hasCitationReviewStatus"),
        ("ontology", "hasFormulaImageEngine"),
        ("ontology", "isEligibleForCodegen"),
        ("validate_host", "Source-mocked formula OCR review"),
        ("python_workflow", "Run Source-mocked formula OCR review"),
        ("matrix", "Phase 6"),
        ("audit", "Phase 6"),
        ("features", "Phase 6"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-formula-ocr-review.py", "scripts/e2e/source-mocked-formula-ocr-review.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase6.formula_ocr_review", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}



def check_phase7_human_workflow() -> dict:
    files = {
        "human_workflow": "services/ingestion/veritas_ingest/human_workflow.py",
        "cli": "services/ingestion/veritas_ingest/cli.py",
        "rust_cli": "apps/cli/src/main.rs",
        "api": "apps/api/src/main.rs",
        "script_py": "scripts/e2e/source-mocked-human-workflow.py",
        "script_sh": "scripts/e2e/source-mocked-human-workflow.sh",
        "schema": "schemas/human_checkpoint.schema.json",
        "opensearch_schema": "schemas/opensearch/evidence_document.schema.json",
        "ontology": "packages/ontology/veritas.owl",
        "validate_host": "scripts/validate-host.sh",
        "python_workflow": ".github/workflows/python.yml",
        "matrix": "VALIDATION_MATRIX.md",
        "audit": "AUDIT.md",
        "features": "FEATURES.md",
    }
    blobs: dict[str, str] = {}
    read_errors: list[str] = []
    for key, rel in files.items():
        value, error = safe_read_text(rel)
        blobs[key] = value
        if error:
            read_errors.append(error)
    required = [
        ("human_workflow", "CHECKPOINT_PHASES"),
        ("human_workflow", "citation_review"),
        ("human_workflow", "formula_review"),
        ("human_workflow", "representation_review"),
        ("human_workflow", "plan_review"),
        ("human_workflow", "code_architecture_review"),
        ("human_workflow", "validation_review"),
        ("human_workflow", "workflow_gate"),
        ("human_workflow", "persist_human_workflow"),
        ("human_workflow", "checkpoints_to_turtle"),
        ("human_workflow", "checkpoint_to_search_record"),
        ("human_workflow", "source_mocked_phase7_summary"),
        ("cli", "review-checkpoint"),
        ("cli", "review-workflow"),
        ("cli", "phase7-source-mocked"),
        ("rust_cli", "ReviewCheckpoint"),
        ("rust_cli", "ReviewWorkflow"),
        ("api", "human_checkpoint_gate_summary"),
        ("api", "human_checkpoints_tail"),
        ("api", "human_checkpoint_gate"),
        ("script_py", "source_mocked_phase7_summary"),
        ("script_py", "phase7-summary.json"),
        ("script_sh", "source-mocked-human-workflow.py"),
        ("schema", "citation_review"),
        ("schema", "code_architecture_review"),
        ("opensearch_schema", "human_checkpoints"),
        ("ontology", "HumanCheckpoint"),
        ("ontology", "hasCheckpointPhase"),
        ("ontology", "blocksWorkflowProgress"),
        ("validate_host", "Source-mocked human workflow"),
        ("python_workflow", "Run Source-mocked human workflow"),
        ("matrix", "Phase 7"),
        ("audit", "Phase 7"),
        ("features", "Phase 7"),
    ]
    missing = [f"{where}:{needle}" for where, needle in required if needle not in blobs.get(where, "")]
    executable = []
    for rel in ["scripts/e2e/source-mocked-human-workflow.py", "scripts/e2e/source-mocked-human-workflow.sh"]:
        path = ROOT / rel
        if path.exists() and not os.access(path, os.X_OK):
            executable.append(rel)
    return {"name": "phase7.human_workflow", "ok": not missing and not read_errors and not executable, "details": {"missing": missing, "read_errors": read_errors, "not_executable": executable}}

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
        check_shell_scripts_executable(),
        check_validation_scope_markers(),
        check_production_acceptance_profiles(),
        check_phase2_source_mocked_e2e(),
        check_phase3_execution_safety(),
        check_phase4_retrieval_ontology_source_mocked(),
        check_phase5_shacl_math_governance(),
        check_phase6_formula_ocr_review(),
        check_phase7_human_workflow(),
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
