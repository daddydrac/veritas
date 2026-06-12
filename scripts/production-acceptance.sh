#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
profile=${VERITAS_ACCEPTANCE_PROFILE:-source-mocked}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) profile=${2:?--profile requires a value}; shift 2 ;;
    --require-live-vllm) export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true; shift ;;
    --skip-live-vllm) export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=false; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$profile" in
  source-mocked|fake-ci)
    export VERITAS_ACCEPTANCE_MODE=mocked_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=true
    export VERITAS_SKIP_DOCKER_VALIDATION=true
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=false
    ;;
  host-prod)
    export VERITAS_ACCEPTANCE_MODE=host_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=${VERITAS_SKIP_CARGO_VALIDATION:-false}
    export VERITAS_SKIP_DOCKER_VALIDATION=${VERITAS_SKIP_DOCKER_VALIDATION:-false}
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}
    ;;
  single-gpu-prod|multi-gpu-prod|live-gpu)
    export VERITAS_ACCEPTANCE_MODE=live_gpu_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=false
    export VERITAS_SKIP_DOCKER_VALIDATION=false
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true
    ;;
  remote-model-prod)
    export VERITAS_ACCEPTANCE_MODE=remote_model_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=false
    export VERITAS_SKIP_DOCKER_VALIDATION=false
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=false
    ;;
  *) echo "Unsupported production acceptance profile: $profile" >&2; exit 2 ;;
esac
echo "Veritas production acceptance profile: $profile ($VERITAS_ACCEPTANCE_MODE)"
scripts/validate-host.sh --profile "$profile"
python3 - <<'PY'
import json, pathlib
validation = pathlib.Path('validation-last.json')
if not validation.exists():
    raise SystemExit('validation-last.json missing after validate-host.sh')
data=json.loads(validation.read_text())
if not data.get('ok'):
    raise SystemExit('validate-spec did not pass')
failed=data.get('summary',{}).get('failed')
if failed != 0:
    raise SystemExit(f'validate-spec failed count={failed}')
host=pathlib.Path('data/e2e/host-validation-summary.json')
if not host.exists():
    raise SystemExit('host validation summary missing')
summary=json.loads(host.read_text())
print('Veritas acceptance gate passed for profile:', summary.get('profile'))
print('Acceptance mode:', summary.get('acceptance_mode'))
PY
