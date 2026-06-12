#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "ingestion"))

from veritas_ingest.formula_ocr_review_contracts import source_mocked_phase6_summary  # noqa: E402

# source_mocked_phase6_summary writes data/e2e/source-mocked-formula-ocr-review/phase6-summary.json

if __name__ == "__main__":
    print(json.dumps(source_mocked_phase6_summary(ROOT), indent=2))
