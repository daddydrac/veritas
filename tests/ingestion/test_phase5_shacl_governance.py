from __future__ import annotations

import json
import os
import pathlib
import subprocess

from rdflib import Graph

from veritas_ingest.shacl_governance_contracts import (
    complete_math_to_code_ttl,
    incomplete_math_artifact_ttl,
    incomplete_symbolic_shadow_ttl,
    invalid_validated_build_ttl,
    load_combined_shape_pack,
    shape_pack_contract,
    shacl_findings_to_turtle,
    source_mocked_phase5_summary,
    validate_math_governance_contract,
)

ROOT = pathlib.Path(__file__).resolve().parents[2]


def test_phase5_combined_shacl_pack_contains_core_and_math_shapes() -> None:
    shapes = load_combined_shape_pack(ROOT)
    contract = shape_pack_contract(shapes)
    assert contract["ok"] is True
    assert contract["has_core_shapes"] is True
    assert contract["has_math_shapes"] is True
    assert "SymbolicShadowExtractionReadinessShape" in shapes
    assert "ProductionBuildArtifactValidationShape" in shapes


def test_phase5_complete_math_to_code_graph_conforms_to_mocked_governance() -> None:
    result = validate_math_governance_contract(complete_math_to_code_ttl())
    assert result["ok"] is True, result
    Graph().parse(data=complete_math_to_code_ttl(), format="turtle")


def test_phase5_symbolic_shadow_missing_obligations_are_blocked() -> None:
    result = validate_math_governance_contract(incomplete_symbolic_shadow_ttl())
    assert result["ok"] is False
    rules = {finding["rule"] for finding in result["findings"]}
    assert "symbolic_shadow.evidence" in rules
    assert "symbolic_shadow.formula_source" in rules
    assert "symbolic_shadow.ocr_status" in rules
    assert "symbolic_shadow.human_review" in rules


def test_phase5_math_artifact_without_representation_invariant_validation_is_blocked() -> None:
    result = validate_math_governance_contract(incomplete_math_artifact_ttl())
    assert result["ok"] is False
    rules = {finding["rule"] for finding in result["findings"]}
    assert "math.representation_map" in rules
    assert "math.validation_requirement" in rules
    assert "math.invariant_or_status" in rules


def test_phase5_validated_build_without_validation_is_blocked() -> None:
    result = validate_math_governance_contract(invalid_validated_build_ttl())
    assert result["ok"] is False
    assert any(finding["rule"] == "build.production_validation" for finding in result["findings"])


def test_phase5_findings_to_rdf_is_parseable() -> None:
    result = validate_math_governance_contract(incomplete_symbolic_shadow_ttl())
    ttl = shacl_findings_to_turtle("test-run", result["findings"])
    graph = Graph().parse(data=ttl, format="turtle")
    assert len(graph) >= result["finding_count"] * 3
    assert "Finding" in ttl


def test_phase5_source_mocked_script_passes() -> None:
    script = ROOT / "scripts/e2e/source-mocked-shacl-governance.sh"
    assert os.access(script, os.X_OK)
    result = subprocess.run([str(script)], cwd=ROOT, text=True, capture_output=True, timeout=60, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["ok"] is True
    names = {check["name"] for check in payload["checks"]}
    assert "combined_shape_pack_loads_core_and_math" in names
    assert "validated_build_without_validation_blocked" in names


def test_phase5_source_summary_direct_call_passes() -> None:
    payload = source_mocked_phase5_summary(ROOT)
    assert payload["ok"] is True
    assert payload["summary"]["checks"] >= 6
