from pathlib import Path
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_all_shell_scripts_are_executable():
    scripts = sorted((ROOT / 'scripts').rglob('*.sh'))
    assert scripts, 'expected shell scripts under scripts/'
    missing = [str(path.relative_to(ROOT)) for path in scripts if not (path.stat().st_mode & 0o111)]
    assert not missing, f'shell scripts must be executable: {missing}'


def test_required_workflows_exist_and_parse():
    workflow_dir = ROOT / '.github' / 'workflows'
    expected = ['python.yml', 'rust.yml', 'docker-e2e.yml']
    for name in expected:
        path = workflow_dir / name
        assert path.exists(), f'missing workflow {path}'
        assert yaml.safe_load(path.read_text()), f'workflow {path} should parse as YAML'


def test_packaging_check_script_passes():
    result = subprocess.run(
        [str(ROOT / 'scripts' / 'check-packaging.sh')],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_acceptance_profiles_are_declared_and_distinct():
    production_acceptance = (ROOT / 'scripts' / 'production-acceptance.sh').read_text()
    validate_host = (ROOT / 'scripts' / 'validate-host.sh').read_text()
    for token in ['source-mocked', 'fake-ci', 'host-prod', 'single-gpu-prod', 'multi-gpu-prod', 'remote-model-prod']:
        assert token in production_acceptance
    assert 'mocked_acceptance' in production_acceptance
    assert 'live_gpu_acceptance' in production_acceptance
    assert 'VERITAS_SKIP_CARGO_VALIDATION' in validate_host
    assert 'VERITAS_SKIP_DOCKER_VALIDATION' in validate_host
    assert 'host-validation-steps.jsonl' in validate_host


def test_validation_matrix_separates_source_and_live_acceptance():
    matrix = (ROOT / 'VALIDATION_MATRIX.md').read_text()
    assert 'Source/mocked acceptance' in matrix
    assert 'Live host acceptance' in matrix
    assert 'Phase 0' in matrix
