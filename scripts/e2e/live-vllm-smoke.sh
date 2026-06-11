#!/usr/bin/env bash
set -euo pipefail
if [ ! -f .veritas/runtime.env ]; then
  echo "Missing .veritas/runtime.env. Run veritas init first." >&2
  exit 1
fi
source .veritas/runtime.env
roles=(PLANNER CODE MATH)
for role in "${roles[@]}"; do
  url_var="VERITAS_${role}_VLLM_URL"
  served_var="VERITAS_${role}_SERVED_MODEL_NAME"
  url="${!url_var:-}"
  served="${!served_var:-}"
  host_url="$url"
  # In host mode, service URLs may be docker-internal. Prefer exposed ports when present.
  if [[ "$url" == http://vllm-* ]]; then
    port_var="VERITAS_${role}_VLLM_PORT"
    port="${!port_var:-}"
    host_url="http://localhost:${port}"
  fi
  echo "Checking $role vLLM at $host_url for served model $served"
  curl -fsS "$host_url/v1/models" | tee "data/e2e/live-vllm-${role,,}-models.json" >/dev/null
  python3 - <<PY
import json, pathlib
p=pathlib.Path('data/e2e/live-vllm-${role,,}-models.json')
data=json.loads(p.read_text())
ids=[m.get('id') for m in data.get('data', [])]
assert '$served' in ids or ids, {'expected':'$served','ids':ids}
print({'role':'$role','ids':ids})
PY
done
