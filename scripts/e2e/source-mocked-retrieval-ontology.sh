#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"
export PYTHONPATH="services/ingestion:${PYTHONPATH:-}"
exec python3 scripts/e2e/source-mocked-retrieval-ontology.py
