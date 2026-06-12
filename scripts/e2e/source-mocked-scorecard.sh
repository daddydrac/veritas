#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"
python3 scripts/generate-feature-scorecard.py --run-validate-spec --update-docs
python3 - <<'PY'
import json, pathlib
payload=json.loads(pathlib.Path('data/scorecard/feature-scorecard.json').read_text())
assert payload['source_mocked_average_score'] >= 94, payload['source_mocked_average_score']
assert payload['source_mocked_all_a_or_b'] is True
assert payload['status'] == 'source_mocked_ready'
assert pathlib.Path('FEATURE_SCORECARD.md').exists()
print('ok=true')
print('phase=phase8_scorecard')
print('source_mocked_average_score=', payload['source_mocked_average_score'])
print('features=', len(payload['features']))
PY
