from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]


def test_fake_embedding_and_vllm_compose_profile_present():
    compose = yaml.safe_load((ROOT / 'docker-compose.e2e.yml').read_text())
    services = compose['services']
    assert 'fake-vllm-planner' in services
    assert 'fake-vllm-code' in services
    assert 'fake-vllm-math' in services
    assert services['embedding']['build']['dockerfile'] == 'Dockerfile.embedding'
    assert services['api']['depends_on']['fake-vllm-planner']['condition'] == 'service_healthy'
    assert services['api']['environment']['VERITAS_REQUIRE_MODELS'] == 'true'


def test_pass5_scripts_are_executable_and_strict():
    scripts = [
        'scripts/e2e/full-fake-vllm-e2e.sh',
        'scripts/e2e/wait-ready.sh',
        'scripts/e2e/ingest-fixture.sh',
        'scripts/e2e/run-fixture.sh',
        'scripts/e2e/gpu-validation.sh',
        'scripts/e2e/live-vllm-smoke.sh',
        'scripts/validate-host.sh',
        'scripts/production-acceptance.sh',
    ]
    for rel in scripts:
        path = ROOT / rel
        text = path.read_text()
        assert 'set -euo pipefail' in text
        assert path.stat().st_mode & 0o111, f'{rel} must be executable'
    validate_host = (ROOT / 'scripts/validate-host.sh').read_text()
    assert 'cargo check --workspace' in validate_host
    assert 'docker compose --env-file .veritas/runtime.env config' in validate_host
    assert 'scripts/e2e/full-fake-vllm-e2e.sh' in validate_host


def test_sample_pdf_fixture_exists_for_docker_e2e():
    fixture = ROOT / 'tests/fixtures/sample_math_paper.pdf'
    data_fixture = ROOT / 'data/fixtures/sample_math_paper.pdf'
    assert fixture.exists() and fixture.stat().st_size > 1000
    assert data_fixture.exists() and data_fixture.stat().st_size > 1000


def test_cli_has_production_proof_commands_and_gpu_validation():
    cli = (ROOT / 'apps/cli/src/main.rs').read_text()
    assert 'E2eFake' in cli
    assert 'ValidateHost' in cli
    assert 'ProductionAccept' in cli
    assert 'detect_gpu_inventory' in cli
    assert 'model_vram_hint_gb' in cli
    assert 'VERITAS_STRICT_GPU_VALIDATE' in cli
