from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def test_phase8_scorecard_generation() -> None:
    result = subprocess.run(
        ["scripts/e2e/source-mocked-scorecard.sh"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
        timeout=120,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    scorecard = json.loads((ROOT / "data/scorecard/feature-scorecard.json").read_text())
    assert scorecard["status"] == "source_mocked_ready"
    assert scorecard["source_mocked_average_score"] >= 94
    assert scorecard["source_mocked_all_a_or_b"] is True
    pending = {f["id"] for f in scorecard["features"] if f["grade"] == "host_validation_pending"}
    assert {"rust_validation", "docker_e2e_validation", "live_vllm_validation"}.issubset(pending)


def test_quickstart_has_source_mocked_and_live_paths() -> None:
    text = (ROOT / "QUICKSTART.md").read_text()
    assert "source-mocked" in text
    assert "FEATURE_SCORECARD.md" in text
    assert "scripts/production-acceptance.sh --profile source-mocked" in text
    assert "single-gpu-prod" in text
    assert ".env.example" not in text


def test_features_links_phase8_scorecard() -> None:
    text = (ROOT / "FEATURES.md").read_text()
    assert "Phase 8" in text
    assert "FEATURE_SCORECARD.md" in text
    assert "source/mocked acceptance" in text
