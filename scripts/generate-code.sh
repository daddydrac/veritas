#!/usr/bin/env bash
set -euo pipefail
PROMPT=${1:?Usage: ./scripts/generate-code.sh "prompt" [language]}
LANGUAGE=${2:-rust}
docker compose run --rm ingestion python -m veritas_ingest.cli generate-code --prompt "$PROMPT" --language "$LANGUAGE"
