from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

validate_path = Path("validation-last.json")
if validate_path.exists():
    raw = validate_path.read_text(encoding="utf-8")
    try:
        data = json.loads(raw) if raw.strip() else {"summary": {"note": "validation-last.json was empty"}}
    except json.JSONDecodeError as exc:
        data = {"summary": {"note": "validation-last.json was not valid JSON", "error": str(exc)}}
else:
    data = {"summary": {"note": "No validation-last.json present; run scripts/validate-host.sh and scripts/validate-spec.py first."}}
section = f"""
\n## Generated audit snapshot\n\nGenerated at: {datetime.now(timezone.utc).isoformat()}\n\n```json\n{json.dumps(data.get('summary', data), indent=2)}\n```\n"""
path = Path("AUDIT.md")
text = path.read_text(encoding="utf-8") if path.exists() else "# Veritas Audit\n"
marker = "\n## Generated audit snapshot\n"
if marker in text:
    text = text.split(marker)[0]
path.write_text(text.rstrip() + section, encoding="utf-8")
print("Updated AUDIT.md")
