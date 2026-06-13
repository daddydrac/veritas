from __future__ import annotations

import json
from pathlib import Path

import jsonschema

ROOT = Path(__file__).resolve().parents[2]


def _read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_planning_context_schema_requires_approved_evidence_contract() -> None:
    schema = json.loads(_read("schemas/planning_context.schema.json"))
    required = set(schema["required"])
    assert {
        "approved_evidence_ids",
        "approved_citation_ids",
        "eligible_formula_ids",
        "allowed_lineage_ids",
        "blocking_reasons",
        "status",
        "production_bound",
    }.issubset(required)

    invalid = {
        "kind": "VeritasPlanningContext",
        "goal": "Build implementation",
        "execution_mode": "production",
        "production_bound": True,
        "status": "ready_for_evidence_backed_planning",
        "ok": True,
        "retrieved_evidence": {},
        "ontology_facts": {},
        "formula_trace": {},
        "allowed_lineage_ids": {"evidence_ids": [], "citation_ids": [], "formula_ids": []},
        "blocking_reasons": [],
    }
    try:
        jsonschema.validate(invalid, schema)
        raise AssertionError("planning_context without approved id arrays should fail")
    except jsonschema.ValidationError:
        pass


def test_application_builds_planning_context_before_model_planning() -> None:
    main = _read("apps/api/src/main.rs")
    assert "planning_context::build" in main
    assert "call_chat_model_json(state, &state.planner_model" in main
    assert main.index("planning_context::build") < main.index("call_chat_model_json(state, &state.planner_model")
    assert "planning_context::validate_plan_references" in main
    assert main.index("planning_context::validate_plan_references") > main.index("call_chat_model_json(state, &state.planner_model")
    assert "planning_context::write_context" in main


def test_planning_context_blocks_production_without_approved_evidence() -> None:
    source = _read("apps/api/src/planning_context.rs")
    assert "planning_context.no_approved_evidence" in source
    assert "Production-bound planning requires approved evidence" in source
    assert "Evidence Eligibility Registry is missing" in source
    assert "approved_citation_ids" in source
    assert "eligible_formula_ids" in source
    assert "VERITAS_ALLOW_EMPTY_EVIDENCE" in source
    assert "dev_only_unverified" in source


def test_dev_empty_evidence_bypass_is_non_production() -> None:
    artifact_decision = _read("apps/api/src/artifact_decision.rs")
    run_report_schema = json.loads(_read("schemas/run_report.schema.json"))
    artifact_schema = json.loads(_read("schemas/artifact_decision.schema.json"))
    assert "DevOnlyUnverified" in artifact_decision
    assert "dev_only_unverified" in artifact_decision
    assert "planning_context.json" in artifact_decision
    assert "dev_only_unverified" in artifact_schema["definitions"]["artifactStatus"]["enum"]
    assert "dev_only_unverified" in run_report_schema["definitions"]["artifactStatus"]["enum"]
