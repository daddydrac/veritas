#!/usr/bin/env bash
set -euo pipefail
mkdir -p data/fixtures data/e2e
if [ ! -f data/fixtures/sample_math_paper.pdf ]; then
  cp tests/fixtures/sample_math_paper.pdf data/fixtures/sample_math_paper.pdf
fi
docker compose --env-file .veritas/runtime.env -f docker-compose.yml -f docker-compose.e2e.yml run --rm ingestion \
  python -m veritas_ingest.cli ingest-pdf \
  --path /workspace/data/fixtures/sample_math_paper.pdf \
  --paper-id sample-math-paper \
  --title "A Minimal Veritas Math Paper" \
  --summary "E2E fixture with a formula symbolic shadow for Veritas validation." | tee data/e2e/ingest-fixture.json
