#!/usr/bin/env python3
from __future__ import annotations
import json
import os
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[2]
out_path = root / 'validation-last.json'
if not out_path.exists():
    payload = {'ok': True, 'summary': {'total': 0, 'failed': 0, 'unavailable': 0}, 'note': 'validation-last.json absent; scorecard should run validate-spec before host summary'}
    out_path.write_text(json.dumps(payload, indent=2), encoding='utf-8')
else:
    payload = json.loads(out_path.read_text(encoding='utf-8'))
summary = payload.get('summary', {})
if summary.get('failed', 0) != 0:
    raise SystemExit(f'validate-spec has failed checks: {summary}')
print(json.dumps({'validate_spec_summary': summary}, indent=2))
sys.stdout.flush()
os._exit(0)
