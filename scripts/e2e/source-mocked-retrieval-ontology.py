#!/usr/bin/env python3
"""Phase 4 source/mocked retrieval + ontology hardening proof.

This harness proves OpenSearch and Fuseki contracts without live services.
It validates mapping shape, migration alias actions, retrieval fallback,
named graph discipline, no-PDF-binary RDF uploads, run-report RDF facts, and
SPARQL planner fact summarization.
"""
from __future__ import annotations

import json
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "services" / "ingestion"))

from veritas_ingest.retrieval_ontology_contracts import source_mocked_phase4_summary  # noqa: E402


def main() -> int:
    payload = source_mocked_phase4_summary()
    out_dir = ROOT / "data" / "e2e" / "source-mocked-retrieval-ontology"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "phase4-summary.json").write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0 if payload["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
