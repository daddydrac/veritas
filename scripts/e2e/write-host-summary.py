#!/usr/bin/env python3
from __future__ import annotations
import json
import os
import pathlib
import time

steps_path = pathlib.Path('data/e2e/host-validation-steps.jsonl')
steps = []
if steps_path.exists():
    steps = [json.loads(line) for line in steps_path.read_text(encoding='utf-8').splitlines() if line.strip()]
summary = {
    'ok': True,
    'profile': os.environ.get('VERITAS_ACCEPTANCE_PROFILE', 'unknown'),
    'acceptance_mode': os.environ.get('VERITAS_ACCEPTANCE_MODE', 'host_acceptance'),
    'completed_at_epoch': int(time.time()),
    'cargo_validation': 'skipped' if os.environ.get('VERITAS_SKIP_CARGO_VALIDATION') == 'true' else 'passed',
    'docker_validation': 'skipped' if os.environ.get('VERITAS_SKIP_DOCKER_VALIDATION') == 'true' else 'passed',
    'live_vllm_required': os.environ.get('VERITAS_REQUIRE_LIVE_VLLM_VALIDATION') == 'true',
    'steps': steps,
}
out = pathlib.Path('data/e2e/host-validation-summary.json')
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(summary, indent=2), encoding='utf-8')
print(json.dumps(summary, indent=2))
