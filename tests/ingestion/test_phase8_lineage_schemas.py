from __future__ import annotations

import json
from pathlib import Path

import jsonschema

ROOT = Path(__file__).resolve().parents[2]


def _schema(name: str) -> dict:
    return json.loads((ROOT / "schemas" / f"{name}.schema.json").read_text(encoding="utf-8"))


def test_planner_schema_requires_step_lineage_fields() -> None:
    schema = _schema("planner")
    step_required = set(schema["properties"]["steps"]["items"]["required"])
    assert {
        "evidence_ids",
        "citation_ids",
        "formula_ids",
        "risk_ids",
        "validation_gate_ids",
        "human_checkpoint_ids",
    }.issubset(step_required)

    invalid = {
        "objective": {"summary": "bad"},
        "steps": [{"id": "code", "tool": "code_generation", "description": "code", "success_criteria": ["files"]}],
        "risks": [{"id": "risk-1", "risk": "bad", "mitigation": "test", "severity": "medium"}],
        "validation_gates": [{"id": "vg-test", "check": "pytest", "command": "python -m pytest"}],
    }
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("planner step without lineage arrays should fail")
    except jsonschema.ValidationError:
        pass


def test_codegen_schema_requires_file_and_command_lineage() -> None:
    schema = _schema("codegen")
    file_required = set(schema["properties"]["files"]["items"]["required"])
    command_required = set(schema["properties"]["commands"]["items"]["required"])
    assert {
        "derived_from_plan_step_ids",
        "derived_from_evidence_ids",
        "derived_from_citation_ids",
        "derived_from_formula_ids",
        "required_validation_ids",
    }.issubset(file_required)
    assert {"derived_from_plan_step_ids", "required_validation_ids"}.issubset(command_required)

    invalid = {
        "package_name": "bad",
        "language": "python",
        "files": [{"path": "src/lib.py", "content": "x=1"}],
        "commands": [{"command": "python -m pytest"}],
    }
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("codegen without lineage should fail")
    except jsonschema.ValidationError:
        pass


def test_run_report_schema_requires_audit_lineage_bundle() -> None:
    schema = _schema("run_report")
    assert schema["additionalProperties"] is False
    assert {
        "source_documents",
        "citations",
        "formulas",
        "review_decisions",
        "representation_model",
        "planning_context",
        "plan_lineage",
        "file_lineage",
        "command_lineage",
        "validation_lineage",
        "repair_lineage",
        "governance_lineage",
    }.issubset(set(schema["required"]))


def test_application_blocks_file_writes_until_codegen_lineage_is_validated() -> None:
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    assert "lineage::validate_codegen_lineage_for_plan" in main
    assert main.index("lineage::validate_codegen_lineage_for_plan") < main.index("write_generated_files")
    assert "lineage::build_report_lineage" in main
    lineage = (ROOT / "apps/api/src/lineage.rs").read_text(encoding="utf-8")
    assert "Code model output failed Veritas lineage enforcement before file writes" in lineage
    assert "derived_from_citation_ids" in lineage
