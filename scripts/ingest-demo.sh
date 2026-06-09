#!/usr/bin/env bash
set -euo pipefail
QUERY=${1:-"cat:cs.AI OR cat:math.OC"}
MAX=${2:-3}
docker compose run --rm ingestion python -m veritas_ingest.cli ingest-arxiv --query "$QUERY" --max-results "$MAX"
