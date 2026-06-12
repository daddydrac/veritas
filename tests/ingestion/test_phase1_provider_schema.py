from __future__ import annotations

import json
from pathlib import Path

import jsonschema

ROOT = Path(__file__).resolve().parents[2]


def _schema(name: str) -> dict:
    return json.loads((ROOT / "schemas" / f"{name}.schema.json").read_text())


def test_phase1_schemas_are_full_json_schema_contracts() -> None:
    for name in ["planner", "codegen", "math_reasoning", "repair", "human_checkpoint", "run_report"]:
        schema = _schema(name)
        jsonschema.Draft7Validator.check_schema(schema)
        assert schema.get("$schema") == "http://json-schema.org/draft-07/schema#"
        assert schema.get("type") == "object"
        assert "required" in schema
    assert _schema("planner")["additionalProperties"] is False
    assert _schema("codegen")["additionalProperties"] is False
    assert _schema("math_reasoning")["additionalProperties"] is False


def test_planner_schema_rejects_unknown_tools_and_missing_fields() -> None:
    schema = _schema("planner")
    valid = {
        "objective": {"summary": "Build tested code"},
        "steps": [
            {"id": "r", "tool": "retrieval", "description": "Retrieve", "input": {}, "success_criteria": ["evidence"]},
            {"id": "c", "tool": "code_generation", "description": "Code", "input": {}, "success_criteria": ["files"]},
            {"id": "t", "tool": "test_runner", "description": "Test", "input": {}, "success_criteria": ["tests"]},
        ],
        "risks": [{"risk": "bad output", "mitigation": "validate"}],
        "validation_gates": [{"check": "cargo test", "command": "cargo test"}],
    }
    jsonschema.validate(valid, schema)
    invalid = dict(valid)
    invalid["steps"] = [{"id": "x", "tool": "delete_world", "description": "bad", "success_criteria": ["bad"]}]
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("unknown planner tool should fail JSON Schema validation")
    except jsonschema.ValidationError:
        pass


def test_codegen_schema_rejects_unsafe_paths_and_extra_properties() -> None:
    schema = _schema("codegen")
    valid = {
        "package_name": "veritas_generated_example",
        "language": "rust",
        "files": [{"path": "src/lib.rs", "content": "pub fn ok() {}"}],
        "commands": [{"command": "cargo test", "purpose": "validate"}],
        "artifact_status": "generated_unvalidated",
    }
    jsonschema.validate(valid, schema)
    for bad_path in ["/tmp/evil.rs", "../evil.rs", "src/../../evil.rs"]:
        invalid = dict(valid)
        invalid["files"] = [{"path": bad_path, "content": "bad"}]
        try:
            jsonschema.validate(invalid, schema)
            raise AssertionError(f"unsafe path should fail schema validation: {bad_path}")
        except jsonschema.ValidationError:
            pass
    invalid = dict(valid)
    invalid["unexpected"] = True
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("additionalProperties=false should reject unexpected fields")
    except jsonschema.ValidationError:
        pass


def test_math_schema_enforces_representation_first_contract() -> None:
    schema = _schema("math_reasoning")
    valid = {
        "summary": "formula is a symbolic shadow",
        "axiom_map": ["A3", "A4"],
        "surface_phenomenon": {"description": "formula text"},
        "representation_hypothesis": "typed pure function",
        "candidate_representation_map": {"map": "R: surface -> latent"},
        "primitive_ontology": [{"entity": "input"}],
        "transformation_space": [{"transformation": "repeat evaluation"}],
        "constraint_geometry": [{"constraint": "no hidden state"}],
        "invariants": [{"name": "referential transparency"}],
        "compression_fidelity": {"preserved": ["semantics"]},
        "recursive_closure": {"behavior": "terminates"},
        "generative_necessity": [{"claim": "tests required"}],
        "symbolic_shadows": [{"expression": "E=mc^2"}],
        "transfer_tests": [{"case": "edge inputs"}],
        "risks": [],
        "validation_requirements": ["unit tests"],
        "status": "plausible",
    }
    jsonschema.validate(valid, schema)
    invalid = dict(valid)
    invalid.pop("candidate_representation_map")
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("math reasoning without representation map must fail")
    except jsonschema.ValidationError:
        pass


def test_provider_router_has_health_retry_circuit_breaker_and_route_history() -> None:
    providers = (ROOT / "apps/api/src/providers.rs").read_text()
    assert "/v1/models" in providers
    assert "openai_compatible_models_health" in providers
    assert "ProviderRetryPolicy" in providers
    assert "circuit_failure_threshold" in providers
    assert "CircuitOpen" in providers
    assert "HalfOpen" in providers
    assert "history_snapshot" in providers
    assert "VERITAS_REMOTE_PLANNER_MODEL" in providers
    assert "VERITAS_REMOTE_CODE_MODEL" in providers
    assert "VERITAS_REMOTE_MATH_MODEL" in providers


def test_api_uses_full_schema_validation_and_exposes_provider_health() -> None:
    main = (ROOT / "apps/api/src/main.rs").read_text()
    schemas = (ROOT / "apps/api/src/schemas.rs").read_text()
    assert "validate_json_schema(schema, value)" in main
    assert "model.schema_invalid" in main
    assert "SchemaKey::HumanCheckpoint" in main
    assert "SchemaKey::RunReport" in main
    assert "validate_model_json(SchemaKey::RunReport" in main
    assert "health_for_role" in main
    assert "provider_route_history" in main
    assert "jsonschema::{Draft, JSONSchema}" in schemas
    assert "additionalProperties" not in schemas or "JSONSchema::options" in schemas


def test_fake_vllm_supports_structured_output_and_negative_modes() -> None:
    fake = (ROOT / "tests/fakes/fake_vllm_server.py").read_text()
    assert "guided_json" in fake
    assert "response_format" in fake
    assert "FAKE_VLLM_RESPONSE_MODE" in fake
    assert "schema_violation" in fake
    assert "unsafe_path" in fake
    assert "/v1/models" in fake
