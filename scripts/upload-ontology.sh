#!/usr/bin/env bash
set -euo pipefail
docker compose --env-file .veritas/runtime.env run --rm ingestion python -m veritas_ingest.cli upload-ontology "$@"
