from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[2]


def test_api_uses_role_specific_schema_guidance():
    src = (ROOT / "apps/api/src/main.rs").read_text()
    providers = (ROOT / "apps/api/src/providers.rs").read_text()
    schemas = (ROOT / "apps/api/src/schemas.rs").read_text()
    assert "SchemaKey::Planner" in src
    assert "SchemaKey::Codegen" in src
    assert "mod providers;" in src
    assert "mod schemas;" in src
    assert "ProviderRouter" in src
    assert 'guided_json"] = plan_schema_description' not in src
    assert "pub trait ModelProvider" in providers
    assert "LocalVllmProvider" in providers
    assert "RemoteOpenAICompatibleProvider" in providers
    assert "schema_json(schema_key)" in providers
    assert 'include_str!("../../../schemas/planner.schema.json")' in schemas


def test_cli_exposes_production_commands():
    src = (ROOT / "apps/cli/src/main.rs").read_text()
    for name in ["MathToCode", "OpenSearchMigrate", "RunStatus", "RunResume", "RunCancel", "GpuInspect", "GpuValidate"]:
        assert name in src


def test_schema_files_are_valid_json():
    for path in (ROOT / "schemas").rglob("*.json"):
        json.loads(path.read_text())


def test_fake_vllm_server_exists():
    assert (ROOT / "tests/fakes/fake_vllm_server.py").exists()
    assert (ROOT / "docker-compose.e2e.yml").exists()


def test_pass2_execution_safety_source_contracts():
    src = (ROOT / "apps/api/src/main.rs").read_text()
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
        "automatic_shacl_report.json",
        "command_audit.jsonl",
        "events.jsonl",
        "next_event_sequence",
        "write_json_file",
        "run.lock",
        "CancelRequested",
    ]
    missing = [needle for needle in required if needle not in src]
    assert not missing, missing
    assert "resume_requires_original_request" not in src
    assert "Automatic step-level resume" not in src
