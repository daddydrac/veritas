#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
chmod +x scripts/*.sh scripts/e2e/*.sh scripts/e2e/*.py || true
scripts/check-packaging.sh
out=${1:-../veritas-release.zip}
rm -f "$out"
# zip preserves Unix executable bits when files have the right mode. Runtime
# artifacts are excluded because validation scripts can regenerate them.
(cd .. && zip -qr "$(realpath "$out")" "$(basename "$ROOT")" \
  -x "$(basename "$ROOT")/.pytest_cache/*" \
  -x "$(basename "$ROOT")/**/__pycache__/*" \
  -x "$(basename "$ROOT")/data/e2e/*" \
  -x "$(basename "$ROOT")/validation-last.json")
echo "Created $out"
