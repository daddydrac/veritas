#!/usr/bin/env python3
from __future__ import annotations
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[2]
proc = subprocess.run([sys.executable, 'scripts/validate-spec.py'], cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
if proc.returncode != 0:
    sys.stderr.write(proc.stderr)
    sys.stderr.write(proc.stdout[-4000:])
    raise SystemExit(proc.returncode)
try:
    payload = json.loads(proc.stdout)
except json.JSONDecodeError as exc:
    pathlib.Path(root / 'data/e2e/validate-spec-raw.txt').write_text(proc.stdout + '\nSTDERR:\n' + proc.stderr, encoding='utf-8')
    raise SystemExit(f'validate-spec did not emit valid JSON: {exc}')
(root / 'validation-last.json').write_text(json.dumps(payload, indent=2), encoding='utf-8')
print(json.dumps({'validate_spec_summary': payload.get('summary', {})}, indent=2))
