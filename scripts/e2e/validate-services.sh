#!/usr/bin/env bash
set -euo pipefail
API_URL=${VERITAS_API_URL:-http://localhost:${VERITAS_API_PORT:-8080}}
mkdir -p data/e2e
for endpoint in health ready models opensearch/status graph/status graph/facts; do
  safe=${endpoint//\//-}
  echo "Checking $endpoint"
  curl -fsS "$API_URL/$endpoint" | tee "data/e2e/${safe}.json" >/dev/null
done
python3 - <<'PY'
import json, pathlib
ready=json.loads(pathlib.Path('data/e2e/ready.json').read_text())
assert ready.get('ready') is True, ready
models=json.loads(pathlib.Path('data/e2e/models.json').read_text())
assert models.get('serving_solution') == 'vllm', models
print('Veritas E2E service checks passed.')
PY
