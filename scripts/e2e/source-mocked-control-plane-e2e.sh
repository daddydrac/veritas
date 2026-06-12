#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"
python3 scripts/e2e/source-mocked-control-plane-e2e.py
OUT_DIR="${VERITAS_SOURCE_MOCKED_E2E_DIR:-data/e2e/source-mocked-control-plane}"
REPORT_PATH="$OUT_DIR/final_report.json"
RESPONSE_PATH="data/e2e/source-mocked-run-response.json"
scripts/e2e/wrap-final-report.py "$REPORT_PATH" "$RESPONSE_PATH"
python3 scripts/e2e/assert-e2e-result.py "$RESPONSE_PATH"
