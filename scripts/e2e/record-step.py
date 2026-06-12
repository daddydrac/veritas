#!/usr/bin/env python3
from __future__ import annotations
import json
import pathlib
import sys
import time

if len(sys.argv) != 5:
    raise SystemExit('usage: record-step.py <name> <status> <details> <jsonl_path>')
name, status, details, path = sys.argv[1:5]
out = pathlib.Path(path)
out.parent.mkdir(parents=True, exist_ok=True)
with out.open('a', encoding='utf-8') as fh:
    fh.write(json.dumps({'name': name, 'status': status, 'details': details, 'epoch': int(time.time())}) + '\n')
