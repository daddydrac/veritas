from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import jsonschema

ROOT = Path(__file__).resolve().parents[2]


def test_phase2_source_mocked_control_plane_e2e_runs_and_validates(tmp_path: Path) -> None:
    out_dir = tmp_path / "source-mocked-control-plane"
    env = os.environ.copy()
    env["VERITAS_SOURCE_MOCKED_E2E_DIR"] = str(out_dir)

    result = subprocess.run(
        ["scripts/e2e/source-mocked-control-plane-e2e.sh"],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        timeout=90,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr

    summary = json.loads((out_dir / "summary.json").read_text())
    report = json.loads((out_dir / "final_report.json").read_text())
    run_report_schema = json.loads((ROOT / "schemas" / "run_report.schema.json").read_text())
    jsonschema.validate(report, run_report_schema)

    assert summary["ok"] is True
    assert summary["final_status"] == "production_candidate_validated"
    assert summary["retry_count"] == 1
    assert all(case["rejected"] is True for case in summary["negative_schema_cases"])

    command_audit = [json.loads(line) for line in (out_dir / "command_audit.jsonl").read_text().splitlines()]
    events = [json.loads(line) for line in (out_dir / "events.jsonl").read_text().splitlines()]
    assert command_audit[0]["exit_code"] != 0
    assert command_audit[-1]["exit_code"] == 0
    assert any(event.get("event") == "repair_applied" for event in events)
    assert (out_dir / "workspace" / "src" / "veritas_generated_example" / "__init__.py").exists()


def test_phase2_validate_spec_includes_source_mocked_check() -> None:
    result = subprocess.run(
        ["python3", "scripts/validate-spec.py"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    checks = {check["name"]: check for check in payload["checks"]}
    assert checks["phase2.source_mocked_control_plane_e2e"]["ok"] is True
