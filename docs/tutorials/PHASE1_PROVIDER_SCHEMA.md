# Phase 1 — Provider Abstraction and Structured-Output Enforcement

Phase 1 hardens the Veritas control plane so model output is treated as untrusted until it passes the role-specific structured-output contract.

## What changed

- `apps/api/src/providers.rs` now uses a production provider layer around vLLM and remote OpenAI-compatible providers.
- `LocalVllmProvider.health` calls the OpenAI-compatible `/v1/models` endpoint and verifies the configured served model is present.
- `ProviderRouter` adds retry policy, bounded backoff, circuit-breaker state, route history, and local-first / remote-explicit fallback.
- Remote fallback can be overridden per role through `VERITAS_REMOTE_PLANNER_MODEL`, `VERITAS_REMOTE_CODE_MODEL`, and `VERITAS_REMOTE_MATH_MODEL`.
- `apps/api/src/schemas.rs` performs full JSON Schema validation with the Rust `jsonschema` crate using Draft 7 before any domain-specific validation runs.
- Planner, codegen, math reasoning, repair, human checkpoint, and run-report schemas are loaded from `schemas/*.schema.json`.
- Fake vLLM can now emit valid responses, invalid JSON, or schema-invalid planner output for contract tests.

## Why this matters

The Veritas business workflow depends on safe model orchestration. A model may plan, reason, write code, or repair failures, but it must not directly control the system. Rust owns the orchestration boundary.

The production rule is:

```text
model output → JSON parse → full schema validation → domain validation → tool/file/command execution
```

No model output may write files, run commands, update Fuseki/OpenSearch, or change artifact status before it passes the relevant schema.

## Provider behavior

The provider stack now supports:

```text
local vLLM first
→ retry retryable failures
→ open circuit after repeated failures
→ optional remote fallback only when explicitly configured
→ record provider route and failure details
```

Useful configuration:

```bash
VERITAS_MODEL_RETRY_ATTEMPTS=3
VERITAS_PROVIDER_RETRY_BASE_DELAY_MS=150
VERITAS_PROVIDER_RETRY_MAX_DELAY_MS=2000
VERITAS_PROVIDER_CIRCUIT_FAILURE_THRESHOLD=3
VERITAS_PROVIDER_CIRCUIT_COOLDOWN_SECS=30

VERITAS_REMOTE_MODEL_ENABLED=false
VERITAS_REMOTE_MODEL_BASE_URL=
VERITAS_REMOTE_MODEL_NAME=
VERITAS_REMOTE_PLANNER_MODEL=
VERITAS_REMOTE_CODE_MODEL=
VERITAS_REMOTE_MATH_MODEL=
VERITAS_REMOTE_MODEL_API_KEY_ENV=VERITAS_REMOTE_MODEL_API_KEY
```

Remote fallback remains opt-in because it may send prompt/evidence context outside the local environment.

## Test coverage added

`tests/ingestion/test_phase1_provider_schema.py` validates:

- planner schema accepts valid tools and rejects unknown tools;
- codegen schema requires file path/content and command structure;
- Rust schema layer uses `jsonschema::JSONSchema` and role-specific schemas;
- provider router contains health checks, retry policy, circuit-breaker behavior, per-role remote model controls, and guided JSON routing;
- fake vLLM can emit valid and invalid structured outputs.

## Acceptance boundary

This phase still excludes live-host checks by design:

```text
cargo fmt/check/test/clippy
Docker Compose E2E execution
live vLLM model loading / GPU smoke validation
```

Those remain live-host acceptance items. Phase 1 proves the source-level schema and provider control plane.
