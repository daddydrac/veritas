#!/usr/bin/env bash
set -euo pipefail
docker compose run --rm ingestion python -m veritas_ingest.cli upload-ontology "$@"
