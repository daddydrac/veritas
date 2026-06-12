#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "ingestion"))

from veritas_ingest.human_workflow import source_mocked_phase7_summary  # noqa: E402

SUMMARY_NAME = "phase7-summary.json"


def main() -> int:
    workspace = Path(os.environ.get("VERITAS_PHASE7_WORKSPACE", ROOT / "data" / "e2e" / "source-mocked-human-workflow"))
    summary = source_mocked_phase7_summary(workspace)
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 0 if summary.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
