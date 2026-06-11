#!/usr/bin/env bash
set -euo pipefail
API_URL=${VERITAS_API_URL:-http://localhost:${VERITAS_API_PORT:-8080}}
mkdir -p data/e2e
curl -fsS -X POST "$API_URL/run" \
  -H 'content-type: application/json' \
  -d '{"goal":"Generate a minimal tested Rust package from fake evidence","language":"rust","max_retries":1}' \
  | tee data/e2e/run-fixture-response.json
python3 scripts/e2e/assert-e2e-result.py data/e2e/run-fixture-response.json
