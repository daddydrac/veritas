#!/usr/bin/env bash
set -euo pipefail
PROMPT=${1:?Usage: ./scripts/generate-code.sh "prompt" [language]}
LANGUAGE=${2:-rust}
docker compose --env-file .veritas/runtime.env run --rm cli run "$PROMPT" --language "$LANGUAGE"
