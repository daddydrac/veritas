#!/usr/bin/env python3
from __future__ import annotations
import json
import pathlib
import sys

if len(sys.argv) != 3:
    raise SystemExit('usage: wrap-final-report.py <final_report.json> <response.json>')
report_path = pathlib.Path(sys.argv[1])
response_path = pathlib.Path(sys.argv[2])
response_path.parent.mkdir(parents=True, exist_ok=True)
report = json.loads(report_path.read_text(encoding='utf-8'))
response_path.write_text(json.dumps({'ok': True, 'final_report': report}, indent=2), encoding='utf-8')
