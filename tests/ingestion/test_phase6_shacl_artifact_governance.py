from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_governance_mode_replaces_advisory_shacl_boolean() -> None:
    main = read("apps/api/src/main.rs")
    governance = read("apps/api/src/governance.rs")
    gates = read("apps/api/src/gates/shacl.rs")
    assert "governance_mode: GovernanceMode" in main
    assert "GovernanceMode::from_env()" in main
    assert "VERITAS_GOVERNANCE_MODE" in governance
    assert "Self::Enforce" in governance
    assert "Self::Advisory" in governance
    assert "Self::Disabled" in governance
    assert "governance_mode.enforces()" in gates
    assert "state.shacl_enforce" not in main


def test_shacl_data_is_built_from_real_workspace_artifacts() -> None:
    main = read("apps/api/src/main.rs")
    assert "collect_artifact_bundle_ttl" in main
    assert "configured_shacl_artifact_files" in main
    assert "VERITAS_SHACL_ARTIFACT_FILES" in main
    assert "evidence_manifest.json" in main
    assert "formula_manifest.json" in main
    assert "citation_manifest.json" in main
    assert "evidence_registry.json" in main
    assert "representation_model.json" in main
    assert "math_validation_report.json" in main
    assert "artifact_bundle_plus_fuseki_construct" in main
    assert "validating artifact bundle only" in main
    assert "validating synthetic plan/run obligations only" not in main


def test_final_shacl_runs_after_validation_and_can_block_status() -> None:
    main = read("apps/api/src/main.rs")
    validation_index = main.index("validation_results.push")
    final_shacl_index = main.index("final_artifact_shacl")
    report_index = main.index('"kind": "VeritasAutonomousRunReport"')
    assert validation_index < final_shacl_index < report_index
    assert "blocked_by_governance" in main
    assert "shacl_report_conforms" in main
    assert '"final_shacl": final_shacl_report' in main


def test_phase6_docs_and_validator_are_updated() -> None:
    assert "PHASE6_SHACL_ARTIFACT_GOVERNANCE" in read("docs/tutorials/PHASE6_SHACL_ARTIFACT_GOVERNANCE.md")
    assert "phase6.shacl_artifact_governance" in read("scripts/validate-spec.py")
    assert "VERITAS_GOVERNANCE_MODE" in read("README.md")
    assert "artifact-based SHACL" in read("FEATURES.md")
