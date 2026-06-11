#!/usr/bin/env bash
set -euo pipefail
API_URL=${VERITAS_API_URL:-http://localhost:${VERITAS_API_PORT:-8080}}
TIMEOUT=${VERITAS_E2E_READY_TIMEOUT_SECS:-240}
SLEEP=${VERITAS_E2E_READY_INTERVAL_SECS:-5}
started=$(date +%s)
while true; do
  if body=$(curl -fsS "$API_URL/ready" 2>/tmp/veritas-ready.err); then
    if python3 - "$body" <<'PY'
import json, sys
body=json.loads(sys.argv[1])
if body.get('ready'):
    print(json.dumps(body, indent=2))
    raise SystemExit(0)
print(json.dumps(body, indent=2))
raise SystemExit(1)
PY
    then
      exit 0
    fi
  fi
  now=$(date +%s)
  if [ $((now-started)) -ge "$TIMEOUT" ]; then
    echo "Veritas readiness timed out after ${TIMEOUT}s." >&2
    echo "Last curl error:" >&2
    cat /tmp/veritas-ready.err >&2 || true
    docker compose --env-file .veritas/runtime.env -f docker-compose.yml -f docker-compose.e2e.yml ps || true
    docker compose --env-file .veritas/runtime.env -f docker-compose.yml -f docker-compose.e2e.yml logs --tail=80 api embedding fake-vllm-planner fake-vllm-code fake-vllm-math || true
    exit 1
  fi
  sleep "$SLEEP"
done
