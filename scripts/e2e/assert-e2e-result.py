#!/usr/bin/env python3
from __future__ import annotations
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
if not data.get('ok'):
    raise SystemExit(f"Run fixture failed: {json.dumps(data, indent=2)}")
report = data.get('final_report') or data
status = report.get('final_status') or report.get('status') or data.get('status')
files = report.get('files_changed') or report.get('files') or []
commands = report.get('commands_run') or report.get('validation_results') or []
if status not in ('production_candidate_validated', 'validated', 'success'):
    raise SystemExit(f"Unexpected final status: {status}\n{json.dumps(report, indent=2)[:2000]}")
if not files:
    raise SystemExit('No files_changed/files were reported by /run.')
if not commands:
    raise SystemExit('No commands/validation results were reported by /run.')
pathlib.Path('data/e2e/final-report-summary.json').write_text(json.dumps({'ok': True, 'status': status, 'files': files, 'commands': commands}, indent=2))
print(json.dumps({'ok': True, 'status': status, 'files_count': len(files), 'commands_count': len(commands)}, indent=2))
