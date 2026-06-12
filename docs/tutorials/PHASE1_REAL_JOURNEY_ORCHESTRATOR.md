# Phase 1 — Real Journey Orchestrator

This phase adds the first production application entrypoint for the real Veritas user journey.

The goal is to stop treating source-mocked scripts as the product path. Source-mocked scripts remain tests and proof harnesses, but the user-facing workflow now starts through the API and CLI journey commands.

## New API endpoints

```text
POST /journey/run
GET  /journey/:run_id/status
POST /journey/:run_id/review
POST /journey/:run_id/resume
GET  /journey/:run_id/report
```

## New CLI commands

```bash
veritas journey run --source paper.pdf --mode local --goal "Implement the method" --language rust
veritas journey status <run_id>
veritas journey review <run_id> --phase plan_review --decision approve
veritas journey resume <run_id>
veritas journey report <run_id>
```

## What Phase 1 does

The journey orchestrator creates one real run workspace and persists:

```text
journey_request.json
source_manifest.json
request.json
journey_state.json
journey_lifecycle.jsonl
journey_report.json
final_report.json
human_checkpoints.jsonl when reviews are recorded
```

It delegates to the existing real autonomous run core instead of using source-mocked scripts.

## What Phase 1 intentionally does not do yet

Phase 1 registers source documents, but it does not yet implement the Phase 2 real local ingestion backend. The source manifest records this honestly as `ingestion_status=not_started_in_phase1`.

## Intended business effect

Users now have one canonical Veritas journey entrypoint. The next phases attach real local ingestion, evidence eligibility gates, pre-codegen approval enforcement, SHACL enforce-mode behavior, math tools, artifact decision logic, and behavior-derived scorecards to this same journey path.
