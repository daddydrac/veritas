#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
if [ "${VERITAS_STRICT_ACCEPTANCE:-true}" = "true" ]; then
  export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}
fi
scripts/validate-host.sh
python3 - <<'PY'
import json, pathlib, sys
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
print('Veritas production acceptance gate passed for this host profile.')
PY
