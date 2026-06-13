#!/usr/bin/env bash
set -euo pipefail
API_URL=${VERITAS_API_URL:-http://localhost:${VERITAS_API_PORT:-8080}}
mkdir -p data/e2e
curl -fsS -X POST "$API_URL/plan" \
  -H 'content-type: application/json' \
  -d '{"goal":"Generate a tested Rust implementation from the fixture formula evidence.","size":5,"execution_mode":"dev_exploratory"}' \
  | tee data/e2e/plan-fixture.json
python3 - <<'PY'
import json, pathlib, sys
p=pathlib.Path('data/e2e/plan-fixture.json')
data=json.loads(p.read_text())
assert data.get('ok') is True, data
assert 'plan' in data, data
print('Plan fixture response is schema-shaped and ok=true.')
PY
