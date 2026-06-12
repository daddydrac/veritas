#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services/ingestion"))

from veritas_ingest.shacl_governance_contracts import source_mocked_phase5_summary  # noqa: E402


def main() -> int:
    payload = source_mocked_phase5_summary(ROOT)
    out_dir = ROOT / "data/e2e/source-mocked-shacl-governance"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "phase5-summary.json").write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(json.dumps(payload, indent=2))
    return 0 if payload.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
