from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_run_core_invokes_gate_engine_before_codegen_and_validation() -> None:
    main = read("apps/api/src/main.rs")
    gate_call = main.index("run_pre_codegen_gates")
    code_prompt = main.index("build_code_generation_prompt")
    file_write = main.index("write_generated_files")
    command_run = main.index("run_command(&workspace")
    assert gate_call < code_prompt < file_write < command_run
    assert "write_pre_codegen_blocked_report" in main
    assert "return Ok(blocked_report);" in main
    assert "files_changed" in main


def test_gate_engine_persists_gate_decisions_and_blocks_files_commands() -> None:
    gates = read("apps/api/src/gates/mod.rs")
    assert "gate_decisions.jsonl" in gates
    assert "pre_codegen_gate_report.json" in gates
    assert "files_written_allowed" in gates
    assert "commands_run_allowed" in gates
    assert "Pre-codegen gates blocked execution" in gates
    assert "final_report.json" in gates
    assert "files_changed" in gates


def test_human_gate_requires_plan_and_code_architecture_before_codegen() -> None:
    human = read("apps/api/src/gates/human.rs")
    assert "plan_review" in human
    assert "code_architecture_review" in human
    assert "VERITAS_PRE_CODEGEN_CHECKPOINTS" in human
    assert "Required pre-codegen human checkpoint is missing" in human
    assert "Record an approve/edit/skip-with-waiver decision" in human


def test_evidence_representation_math_and_shacl_gates_are_causal() -> None:
    evidence = read("apps/api/src/gates/evidence.rs")
    representation = read("apps/api/src/gates/representation.rs")
    math_tools = read("apps/api/src/gates/math_tools.rs")
    shacl = read("apps/api/src/gates/shacl.rs")
    assert "planning_gate_from_workspace" in evidence
    assert "awaiting_evidence_review" in evidence
    assert "representation_model.json" in representation
    assert "awaiting_representation_review" in representation
    assert "math_validation_report.json" in math_tools
    assert "blocked_by_math_tools" in math_tools
    assert "blocked_by_governance" in shacl
    assert "pre_codegen_shacl" in shacl


def test_journey_metadata_uses_phase4_real_product_path() -> None:
    journey = read("apps/api/src/journey.rs")
    assert "phase4_pre_execution_gate_engine" in journey
    assert "real_product_path" in journey
    assert "mocked" in journey
