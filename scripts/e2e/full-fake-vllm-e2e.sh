#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"
if [ ! -f .veritas/runtime.env ] || [ "${VERITAS_E2E_REWRITE_ENV:-true}" = "true" ]; then
  scripts/e2e/write-fake-runtime-env.sh
fi
mkdir -p data/e2e
COMPOSE=(docker compose --env-file .veritas/runtime.env -f docker-compose.yml -f docker-compose.e2e.yml)
cleanup() {
  if [ "${VERITAS_E2E_KEEP_RUNNING:-false}" != "true" ]; then
    "${COMPOSE[@]}" down --remove-orphans || true
  fi
}
trap cleanup EXIT
"${COMPOSE[@]}" up -d --build
scripts/e2e/wait-ready.sh
scripts/e2e/validate-services.sh
curl -fsS -X POST http://localhost:${VERITAS_API_PORT:-8080}/opensearch/migrate -H 'content-type: application/json' -d '{"dry_run":false,"force_alias_update":true}' | tee data/e2e/opensearch-migrate.json
scripts/e2e/upload-ontology.sh
scripts/e2e/ingest-fixture.sh
scripts/e2e/plan-fixture.sh
scripts/e2e/run-fixture.sh
python3 - <<'PY'
import json, pathlib, time
summary={
  'ok': True,
  'completed_at_epoch': int(time.time()),
  'artifacts': sorted(str(p) for p in pathlib.Path('data/e2e').glob('*.json')),
}
pathlib.Path('data/e2e/fake-vllm-e2e-summary.json').write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
PY
