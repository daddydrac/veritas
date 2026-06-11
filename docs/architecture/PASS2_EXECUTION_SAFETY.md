# Pass 2 — Execution Safety

Pass 2 hardens Veritas run execution so autonomous code generation is resumable, cancellable, and auditable.

## Implemented behavior

- Every `/run` creates a durable workspace under `VERITAS_RUNS_DIR`.
- The original request is persisted as `request.json`.
- Current state is persisted as `state.json`.
- Every state transition appends to `events.jsonl` with a sequence number.
- `run.lock` is acquired with atomic `create_new(true)` semantics so two workers cannot advance the same run concurrently.
- Stale locks are recoverable through `VERITAS_RUN_LOCK_STALE_SECS`.
- Resume reloads `request.json` and reuses `plan_envelope.json`, `tool_outputs.json`, and `automatic_shacl_report.json` when present.
- Generated code packages, changed files, command results, validation results, and retry history are persisted during the run.
- Commands append to `command_audit.jsonl`.
- Cancellation writes `CANCELLED` and records a `CancelRequested` event.

## API endpoints

```text
GET  /status
GET  /status/:run_id
POST /run/:run_id/resume
POST /run/:run_id/cancel
```

## Workspace files

```text
request.json
state.json
events.jsonl
plan_envelope.json
plan.json
tool_calls.json
tool_outputs.json
automatic_shacl_report.json
code_package_attempt_<n>.json
code_package_latest.json
files_changed.json
commands_run.json
command_audit.jsonl
validation_results.json
retry_history.json
final_report.json
```

## Remaining validation boundary

The source-level implementation is complete for Pass 2, but live proof still requires Cargo and Docker host validation.
