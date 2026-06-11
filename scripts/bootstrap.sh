#!/usr/bin/env bash
set -euo pipefail
cat <<'LOGO'
██╗   ██╗███████╗██████╗ ██╗████████╗ █████╗ ███████╗
██║   ██║██╔════╝██╔══██╗██║╚══██╔══╝██╔══██╗██╔════╝
██║   ██║█████╗  ██████╔╝██║   ██║   ███████║███████╗
╚██╗ ██╔╝██╔══╝  ██╔══██╗██║   ██║   ██╔══██║╚════██║
 ╚████╔╝ ███████╗██║  ██║██║   ██║   ██║  ██║███████║
  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚══════╝
LOGO
printf 'Math heavy evidence backed research and development software engineering agent.\n\n'
if [ ! -f .veritas/runtime.env ]; then
  printf '[veritas] no .veritas/runtime.env found. Run the CLI setup wizard first:\n  docker compose run --rm cli init\n' >&2
  exit 2
fi
COMPOSE=(docker compose --env-file .veritas/runtime.env)
printf '[veritas] starting core Docker Compose services...\n'
"${COMPOSE[@]}" up -d --build opensearch fuseki shacl embedding api
API_PORT=$(grep -E '^VERITAS_API_PORT=' .veritas/runtime.env | tail -1 | cut -d= -f2-)
API_PORT=${API_PORT:-8080}
printf '[veritas] waiting for API readiness on http://localhost:%s/ready ...\n' "$API_PORT"
for i in $(seq 1 90); do
  if curl -fsS "http://localhost:${API_PORT}/ready" >/tmp/veritas-ready.json 2>/dev/null; then
    if grep -q '"ready":true' /tmp/veritas-ready.json; then
      printf '[veritas] core services ready.\n'
      break
    fi
  fi
  if [ "$i" -eq 90 ]; then
    printf '[veritas] core stack did not become ready in time.\n' >&2
    printf 'Run: docker compose --env-file .veritas/runtime.env ps && docker compose --env-file .veritas/runtime.env logs --tail=200\n' >&2
    cat /tmp/veritas-ready.json 2>/dev/null || true
    exit 1
  fi
  sleep 5
done
printf '[veritas] uploading Veritas OWL ontology to Fuseki...\n'
"${COMPOSE[@]}" run --rm ingestion python -m veritas_ingest.cli upload-ontology
printf '\nVeritas core is ready. Start vLLM roles as needed:\n'
printf '  docker compose --env-file .veritas/runtime.env --profile models --profile code-model --profile math-model up -d\n'
printf '\nTry:\n'
printf '  docker compose --env-file .veritas/runtime.env run --rm cli models\n'
printf '  docker compose --env-file .veritas/runtime.env run --rm cli ingest-arxiv --query "cat:cs.AI OR cat:math.OC" --max-results 3\n'
printf '  docker compose --env-file .veritas/runtime.env run --rm cli run "turn indexed research into tested Rust code" --language rust\n'
