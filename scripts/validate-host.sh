#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
mkdir -p data/e2e
summary_file=data/e2e/host-validation-summary.json
run_step() {
  local name="$1"; shift
  echo "== $name =="
  "$@"
}
python3 - <<'PY'
import json, pathlib, time
pathlib.Path('data/e2e').mkdir(parents=True, exist_ok=True)
pathlib.Path('data/e2e/host-validation-start.json').write_text(json.dumps({'started_at_epoch': int(time.time())}, indent=2))
PY
run_step "Python compile" python3 -m compileall services/embedding services/ingestion services/shacl tests/fakes
run_step "Python tests" env PYTHONPATH=services/ingestion pytest -q tests/ingestion
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required for production host validation but is unavailable." >&2
  exit 2
fi
run_step "cargo fmt" cargo fmt --all -- --check
run_step "cargo check" cargo check --workspace
run_step "cargo test" cargo test --workspace
run_step "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required for production host validation but is unavailable." >&2
  exit 2
fi
if [ ! -f .veritas/runtime.env ]; then
  scripts/e2e/write-fake-runtime-env.sh
fi
run_step "docker compose config" docker compose --env-file .veritas/runtime.env config
run_step "GPU layout validation" scripts/e2e/gpu-validation.sh
run_step "fake-vLLM Docker E2E" scripts/e2e/full-fake-vllm-e2e.sh
if [ "${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}" = "true" ]; then
  run_step "live vLLM model smoke" scripts/e2e/live-vllm-smoke.sh
else
  echo "Skipping live vLLM smoke. Set VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true on a GPU host to require it."
fi
python3 scripts/validate-spec.py | tee validation-last.json
python3 scripts/update-audit.py
python3 - <<'PY'
import json, pathlib, time
summary={
  'ok': True,
  'completed_at_epoch': int(time.time()),
  'cargo': 'passed',
  'docker_compose_config': 'passed',
  'fake_vllm_e2e': 'passed',
  'live_vllm_required': False,
}
pathlib.Path('data/e2e/host-validation-summary.json').write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
PY
