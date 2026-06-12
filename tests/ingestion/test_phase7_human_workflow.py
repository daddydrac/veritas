from __future__ import annotations

import json
import os
import pathlib
import subprocess

from rdflib import Graph

from veritas_ingest.human_workflow import (
    CHECKPOINT_PHASES,
    build_workflow_checkpoints,
    checkpoints_to_turtle,
    create_checkpoint,
    persist_human_workflow,
    source_mocked_phase7_summary,
    workflow_gate,
)

ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_phase7_require_all_approved_workflow_allows_production(tmp_path: pathlib.Path) -> None:
    checkpoints = build_workflow_checkpoints(policy="require_all")
    gate = workflow_gate(checkpoints, policy="require_all")
    report = persist_human_workflow(tmp_path, checkpoints, gate)
    assert gate["can_continue"] is True
    assert gate["production_status_allowed"] is True
    assert len(report["human_checkpoints"]) == len(CHECKPOINT_PHASES)
    assert pathlib.Path(report["events_path"]).exists()
    assert pathlib.Path(report["rdf_path"]).exists()


def test_phase7_missing_required_plan_review_blocks_progress() -> None:
    checkpoints = [c for c in build_workflow_checkpoints(policy="require_all") if c["phase"] != "plan_review"]
    gate = workflow_gate(checkpoints, policy="require_all")
    assert gate["can_continue"] is False
    assert "plan_review" in gate["missing_phases"]
    assert "plan_review" in gate["blocked_phases"]


def test_phase7_rejected_formula_review_blocks_codegen() -> None:
    checkpoints = build_workflow_checkpoints(policy="require_all", decisions={"formula_review": "reject"})
    gate = workflow_gate(checkpoints, policy="require_all")
    assert gate["can_continue"] is False
    assert "formula_review" in gate["rejected_phases"]


def test_phase7_explicit_waiver_with_reason_satisfies_required_gate() -> None:
    checkpoints = build_workflow_checkpoints(
        policy="require_all",
        decisions={"plan_review": "skip"},
        notes={"plan_review": "External architecture board approved this plan under ADR-7."},
    )
    gate = workflow_gate(checkpoints, policy="require_all")
    assert gate["can_continue"] is True
    assert "plan_review" in gate["waived_phases"]


def test_phase7_checkpoint_rdf_and_search_contract(tmp_path: pathlib.Path) -> None:
    checkpoints = build_workflow_checkpoints(policy="require_all")
    turtle = checkpoints_to_turtle(checkpoints)
    graph = Graph().parse(data=turtle, format="turtle")
    assert len(graph) > 0
    report = persist_human_workflow(tmp_path, checkpoints, workflow_gate(checkpoints, policy="require_all"))
    search_records = pathlib.Path(report["search_records_path"]).read_text(encoding="utf-8").splitlines()
    assert len(search_records) == len(CHECKPOINT_PHASES)
    first = json.loads(search_records[0])
    assert first["doc_type"] == "human_checkpoint"


def test_phase7_create_checkpoint_rejects_unknown_phase() -> None:
    try:
        create_checkpoint(phase="unknown", artifact={}, policy="require_all", decision="approve")
    except ValueError as exc:
        assert "Unsupported checkpoint phase" in str(exc)
    else:
        raise AssertionError("unknown phase should have failed")


def test_phase7_source_mocked_summary_passes(tmp_path: pathlib.Path) -> None:
    summary = source_mocked_phase7_summary(tmp_path)
    assert summary["ok"] is True, summary
    names = {check["name"] for check in summary["checks"]}
    assert "require_all_approved_allows_progress" in names
    assert "missing_required_plan_blocks_progress" in names
    assert "rejected_formula_blocks_codegen" in names
    assert summary["rdf_triples"] > 0


def test_phase7_source_mocked_script_passes() -> None:
    script = ROOT / "scripts/e2e/source-mocked-human-workflow.sh"
    assert os.access(script, os.X_OK)
    result = subprocess.run([str(script)], cwd=ROOT, text=True, capture_output=True, timeout=60, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["ok"] is True
    assert payload["phase"] == "phase7_human_workflow"
