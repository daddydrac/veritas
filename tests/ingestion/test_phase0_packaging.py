from pathlib import Path
import os
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_all_shell_scripts_are_executable():
    scripts = sorted((ROOT / "scripts").rglob("*.sh"))
    assert scripts, "expected shell scripts under scripts/"
    not_executable = [str(path.relative_to(ROOT)) for path in scripts if not os.access(path, os.X_OK)]
    assert not not_executable, "all shell scripts must be executable so ZIP artifacts run without chmod: " + ", ".join(not_executable)


def test_required_ci_workflows_exist_and_parse():
    workflows = [
        ROOT / ".github/workflows/python.yml",
        ROOT / ".github/workflows/rust.yml",
        ROOT / ".github/workflows/docker-e2e.yml",
    ]
    for workflow in workflows:
        assert workflow.exists(), f"missing workflow: {workflow.relative_to(ROOT)}"
        assert yaml.safe_load(workflow.read_text()), f"workflow did not parse: {workflow.relative_to(ROOT)}"


def test_check_packaging_script_passes():
    result = subprocess.run(["bash", "scripts/check-packaging.sh"], cwd=ROOT, text=True, capture_output=True, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_validation_matrix_separates_source_and_live_host_acceptance():
    text = (ROOT / "VALIDATION_MATRIX.md").read_text()
    for needle in ["source/mocked acceptance", "live host acceptance", "host_validation_pending", "cargo.check", "docker.compose.config", "live_vllm_smoke"]:
        assert needle in text


def test_production_acceptance_profiles_are_documented_in_script():
    text = (ROOT / "scripts/production-acceptance.sh").read_text()
    for needle in ["fake-ci", "single-gpu-prod", "multi-gpu-prod", "remote-model-prod", "mocked_acceptance", "live_gpu_acceptance"]:
        assert needle in text
