from __future__ import annotations

import json
from pathlib import Path
from datetime import datetime, timezone

validate_path = Path("validation-last.json")
if validate_path.exists():
    data = json.loads(validate_path.read_text())
else:
    data = {"summary": {"note": "No validation-last.json present; run scripts/validate-host.sh and scripts/validate-spec.py first."}}
section = f"""
\n## Generated audit snapshot\n\nGenerated at: {datetime.now(timezone.utc).isoformat()}\n\n```json\n{json.dumps(data.get('summary', data), indent=2)}\n```\n"""
path = Path("AUDIT.md")
text = path.read_text() if path.exists() else "# Veritas Audit\n"
marker = "\n## Generated audit snapshot\n"
if marker in text:
    text = text.split(marker)[0]
path.write_text(text.rstrip() + section)
print("Updated AUDIT.md")
