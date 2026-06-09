from __future__ import annotations

import json
from pathlib import Path

import pytest

from veritas_ingest import codegen


class _DummyEvidence:
    opensearch_hits = [{"chunk_id": "c1", "title": "Synthetic", "score": 1.0}]
    formula_bindings = [{"expr": "E=mc^2"}]
    graph_bindings = []
    warnings = []


def test_generate_package_writes_review_gated_rust_scaffold(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    cfg = {
        "retrieval": {"top_k": 2},
        "codegen": {"package_outputs_dir": str(tmp_path)},
    }

    def fake_plan(prompt: str, cfg: dict, *, size: int = 8) -> dict:
        return {
            "ok": True,
            "evidence": {
                "opensearch_hits": _DummyEvidence.opensearch_hits,
                "formula_bindings": _DummyEvidence.formula_bindings,
                "graph_bindings": [],
                "warnings": [],
            },
            "analysis": {
                "surface_phenomenon": {"apparent_complexity": ["math"]},
                "representation_hypothesis": {"candidate_map": "surface to latent"},
                "candidate_invariants": ["preserve formula"],
                "risk_register": [{"risk": "MathematicalRisk"}],
                "validation_gates": ["tests_required"],
                "compression_fidelity_gates": ["formula boundaries"],
            },
        }

    monkeypatch.setattr(codegen, "build_evidence_backed_plan", fake_plan)
    result = codegen.generate_package("Implement the method", "rust", cfg)
    out = Path(result["path"])
    assert (out / "Cargo.toml").exists()
    assert (out / "src/lib.rs").exists()
    assert (out / "VALIDATION_REPORT.md").exists()
    manifest = json.loads((out / "veritas_manifest.json").read_text())
    assert manifest["status"] == "generated_scaffold_requires_review"
