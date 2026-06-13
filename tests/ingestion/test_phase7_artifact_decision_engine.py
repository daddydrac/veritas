from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def test_artifact_decision_engine_source_contract() -> None:
    source = (ROOT / "apps/api/src/artifact_decision.rs").read_text(encoding="utf-8")
    main = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    gates = (ROOT / "apps/api/src/gates/mod.rs").read_text(encoding="utf-8")

    assert "enum ArtifactStatus" in source
    assert "LocalValidatedHostPending" in source
    assert "ProductionValidated" in source
    assert "decide_completed_run" in source
    assert "decide_blocked_pre_codegen" in source
    assert "Host validation has not passed" in source
    assert "application_artifact_decision_engine" in source
    assert "artifact_decision::decide_completed_run" in main
    assert 'workspace.join("CANCELLED").exists()' in main
    assert '"artifact_decision": artifact_decision' in main
    assert '"final_status": artifact_status' in main
    assert "decide_blocked_pre_codegen" in gates
    assert '"commands_run": []' in gates
    assert '"files_changed": []' in gates


def test_artifact_decision_schema_requires_canonical_fields() -> None:
    schema = json.loads((ROOT / "schemas/artifact_decision.schema.json").read_text(encoding="utf-8"))
    required = set(schema["required"])
    assert {
        "ok",
        "run_id",
        "artifact_status",
        "final_status",
        "validation_status",
        "production_status_allowed",
        "host_validation_status",
        "decision_factors",
        "remaining_limitations",
        "decision_source",
    }.issubset(required)
    statuses = set(schema["definitions"]["artifactStatus"]["enum"])
    assert "awaiting_human_approval" in statuses
    assert "blocked_by_governance" in statuses
    assert "validation_failed" in statuses
    assert "repair_failed" in statuses
    assert "local_validated_host_pending" in statuses
    assert "production_candidate_validated" in statuses
    assert "production_validated" in statuses


def test_run_report_schema_requires_artifact_decision() -> None:
    schema = json.loads((ROOT / "schemas/run_report.schema.json").read_text(encoding="utf-8"))
    assert "artifact_decision" in schema["required"]
    statuses = set(schema["definitions"]["artifactStatus"]["enum"])
    assert "local_validated_host_pending" in statuses
    assert "production_validated" in statuses
    assert "awaiting_human_approval" in statuses
