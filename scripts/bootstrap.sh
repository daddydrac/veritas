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
cp -n .env.example .env || true
printf '[veritas] starting Docker Compose services...\n'
docker compose up -d --build opensearch fuseki qdrant embedding api
API_PORT=${VERITAS_API_PORT:-8080}
printf '[veritas] waiting for API readiness on http://localhost:%s/ready ...\n' "$API_PORT"
for i in $(seq 1 90); do
  if curl -fsS "http://localhost:${API_PORT}/ready" >/tmp/veritas-ready.json 2>/dev/null; then
    if grep -q '"ready":true' /tmp/veritas-ready.json; then
      printf '[veritas] services ready.\n'
      break
    fi
  fi
  if [ "$i" -eq 90 ]; then
    printf '[veritas] stack did not become ready in time.\n' >&2
    printf 'Run: docker compose ps && docker compose logs --tail=200\n' >&2
    cat /tmp/veritas-ready.json 2>/dev/null || true
    exit 1
  fi
  sleep 5
done
printf '[veritas] uploading Veritas OWL ontology to Fuseki...\n'
docker compose run --rm ingestion python -m veritas_ingest.cli upload-ontology
printf '\nVeritas is ready. Try:\n'
printf '  docker compose run --rm cli welcome\n'
printf '  docker compose run --rm cli ingest-arxiv --query "cat:cs.AI OR cat:math.OC" --max-results 3\n'
printf '  docker compose run --rm cli ingest-pdf --path ./paper.pdf\n'
printf '  docker compose run --rm cli ask "turn indexed research into tested Rust code"\n'
printf '  docker compose run --rm cli generate-code --language rust --prompt "implement the strongest indexed method"\n'
