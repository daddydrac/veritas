#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
missing=$(find scripts -type f -name '*.sh' ! -perm /111 -print | sort)
if [ -n "$missing" ]; then
  echo "The following shell scripts are not executable:" >&2
  echo "$missing" >&2
  echo "Run: chmod +x scripts/*.sh scripts/e2e/*.sh" >&2
  exit 1
fi
for workflow in .github/workflows/python.yml .github/workflows/rust.yml .github/workflows/docker-e2e.yml; do
  if [ ! -f "$workflow" ]; then
    echo "Missing required CI workflow: $workflow" >&2
    exit 1
  fi
done
python3 - <<'PY'
import yaml
from pathlib import Path
for f in [Path('.github/workflows/python.yml'), Path('.github/workflows/rust.yml'), Path('.github/workflows/docker-e2e.yml')]:
    yaml.safe_load(f.read_text())
print('Packaging check passed: shell scripts executable and workflows parse.')
PY
