from __future__ import annotations

import json
import sys
from pathlib import Path

from fastapi.testclient import TestClient

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from services.math_tools.app import app, FormulaPayload, NumericValidatePayload, parse_latex_endpoint, numeric_validate_endpoint



def test_math_tools_parse_and_numeric_validate_real_sympy_logic() -> None:
    parsed = parse_latex_endpoint(FormulaPayload(latex="E = m c^2"))
    assert parsed["ok"] is True
    assert parsed["status"] == "passed"
    assert "normalized_expression" in parsed["result"]
    assert set(parsed["result"]["free_symbols"]) >= {"E", "m", "c"}

    valid = numeric_validate_endpoint(NumericValidatePayload(latex="x = x", samples=5))
    assert valid["ok"] is True
    assert valid["blocks_codegen"] is False
    assert valid["result"]["failure_count"] == 0

    invalid = numeric_validate_endpoint(NumericValidatePayload(latex="x = x + 1", samples=5))
    assert invalid["ok"] is False
    assert invalid["blocks_codegen"] is True
    assert invalid["result"]["failure_count"] > 0


def test_math_tools_validate_endpoint_returns_blocking_report_for_bad_formula() -> None:
    client = TestClient(app)
    response = client.post("/validate", json={"latex": "x = x + 1", "metadata": {"tool_sequence": "parse_latex,numeric_validate,counterexample_search"}})
    assert response.status_code == 200
    body = response.json()
    assert body["kind"] == "VeritasMathValidationReport"
    assert body["ok"] is False
    assert body["blocking_findings"]
    assert body["counterexamples"]


def test_phase5_application_code_contains_real_math_engine_integration() -> None:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    provider = (ROOT / "apps/api/src/providers.rs").read_text(encoding="utf-8")
    compose = (ROOT / "docker-compose.yml").read_text(encoding="utf-8")
    assert "mod math_tools;" in main
    assert "math_tools::validate_workspace_if_required" in main
    assert 'route("/math-tools/status"' in main
    assert 'route("/math-tools/validate"' in main
    assert "pub tools: Option<Value>" in provider
    assert "payload[\"tools\"]" in provider
    assert "math-tools:" in compose
    assert "services/math_tools/Dockerfile" in compose


def test_tool_schemas_are_present_and_valid_json() -> None:
    schema_dir = ROOT / "schemas/tools"
    names = ["parse_latex", "numeric_validate", "counterexample_search", "generate_property_tests"]
    for name in names:
        for suffix in ["input", "output"]:
            path = schema_dir / f"{name}.{suffix}.schema.json"
            assert path.exists(), path
            assert json.loads(path.read_text(encoding="utf-8"))["type"] == "object"
    report = json.loads((ROOT / "schemas/math_validation_report.schema.json").read_text(encoding="utf-8"))
    assert "tool_results" in report["required"]
