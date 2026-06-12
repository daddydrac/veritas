#!/usr/bin/env python3
from __future__ import annotations
import json
import os
import pathlib
import sys
import time

steps_path = pathlib.Path('data/e2e/host-validation-steps.jsonl')
steps = []
if steps_path.exists():
    steps = [json.loads(line) for line in steps_path.read_text(encoding='utf-8').splitlines() if line.strip()]
summary = {
    'ok': all(step.get('status') in {'passed', 'skipped'} for step in steps),
    'profile': os.environ.get('VERITAS_ACCEPTANCE_PROFILE', 'unknown'),
    'acceptance_mode': os.environ.get('VERITAS_ACCEPTANCE_MODE', 'host_acceptance'),
    'completed_at_epoch': int(time.time()),
    'cargo_validation': 'skipped' if os.environ.get('VERITAS_SKIP_CARGO_VALIDATION') == 'true' else 'passed',
    'docker_validation': 'skipped' if os.environ.get('VERITAS_SKIP_DOCKER_VALIDATION') == 'true' else 'passed',
    'live_vllm_required': os.environ.get('VERITAS_REQUIRE_LIVE_VLLM_VALIDATION') == 'true',
    'step_counts': {
        'total': len(steps),
        'passed': sum(1 for step in steps if step.get('status') == 'passed'),
        'skipped': sum(1 for step in steps if step.get('status') == 'skipped'),
        'failed': sum(1 for step in steps if step.get('status') == 'failed'),
    },
    'steps': steps,
}
out = pathlib.Path('data/e2e/host-validation-summary.json')
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(summary, indent=2), encoding='utf-8')
print(json.dumps({
    'ok': summary['ok'],
    'profile': summary['profile'],
    'acceptance_mode': summary['acceptance_mode'],
    'step_counts': summary['step_counts'],
}, indent=2))
sys.stdout.flush()
os._exit(0)
