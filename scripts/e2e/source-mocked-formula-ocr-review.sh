#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
PYTHONPATH=services/ingestion python3 scripts/e2e/source-mocked-formula-ocr-review.py
