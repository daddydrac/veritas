from __future__ import annotations

import json
import sys
from pathlib import Path

from fastapi.testclient import TestClient

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from services.math_tools.app import app  # noqa: E402


def test_math_tools_service_executes_real_symbolic_and_numeric_tools() -> None:
    client = TestClient(app)
    health = client.get("/health")
    assert health.status_code == 200
    assert health.json()["ok"] is True
    assert "parse_latex" in health.json()["tools"]

    parse = client.post("/tools/parse_latex", json={"expression": "x**2 + 2*x + 1", "variables": ["x"]})
    assert parse.status_code == 200
    assert parse.json()["ok"] is True
    assert parse.json()["blocks_codegen"] is False

    numeric = client.post("/tools/numeric_validate", json={"expression": "x**2 + 1", "variables": ["x"], "samples": 5})
    assert numeric.status_code == 200
    assert numeric.json()["ok"] is True
    assert numeric.json()["result"]["counterexamples"] == []


def test_math_tools_validation_blocks_false_equation() -> None:
    client = TestClient(app)
    result = client.post("/validate", json={"expression": "x = x + 1", "variables": ["x"], "formula_id": "bad-formula"})
    assert result.status_code == 200
    payload = result.json()
    assert payload["kind"] == "VeritasMathValidationReport"
    assert payload["ok"] is False
    assert payload["status"] == "blocked_by_math_tools"
    assert payload["blocking_findings"]


def test_phase5_application_logic_wires_math_tools_before_codegen() -> None:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    math_tools = (ROOT / "apps/api/src/math_tools.rs").read_text(encoding="utf-8")
    providers = (ROOT / "apps/api/src/providers.rs").read_text(encoding="utf-8")
    compose = (ROOT / "docker-compose.yml").read_text(encoding="utf-8")

    assert "math_tools::validate_workspace_if_required" in main
    assert main.index("math_tools::validate_workspace_if_required") < main.index("gates::run_pre_codegen_gates")
    assert "math_validation_report.json" in math_tools
    assert "validate_formula_context" in math_tools
    assert "pub tools: Option<Value>" in providers
    assert "payload[\"tools\"]" in providers
    assert "math-tools:" in compose


def test_tool_schemas_exist_and_require_real_report_fields() -> None:
    schema = json.loads((ROOT / "schemas/tools/math_validation_report.schema.json").read_text(encoding="utf-8"))
    assert "kind" in schema["required"]
    assert "tool_results" in schema["required"]
    assert "blocking_findings" in schema["required"]
