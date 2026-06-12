# Phase 3 — Execution Safety Hardening

Phase 3 hardens Veritas generated-code execution without requiring live Docker execution in the source/mocked validation profile. Docker remains the production runner, but this phase proves the policy, path-safety, lock, state, and resume contracts through source/mocked tests.

## Goals

Phase 3 implements the Jira/tutorial requirements for execution safety:

1. Production profiles default to the sandbox runner.
2. Local shell execution is blocked in production unless explicitly allowed.
3. Generated file paths are canonicalized and constrained to the run workspace.
4. Existing symlink parents and symlink targets are rejected before file writes.
5. Docker sandbox command construction includes network isolation, read-only root, capability drop, no-new-privileges, pids limit, memory limit, CPU limit, and tmpfs `/tmp`.
6. Command policy rejects dangerous shell/system tokens before execution.
7. Run state is indexed in `run_index.jsonl` for restart-friendly status discovery.
8. `/status/:run_id` includes command audit and lock metadata.
9. Source/mocked tests prove crash/resume decision points and cancellation blocking.

## Production runner behavior

When `VERITAS_PROFILE` or `VERITAS_ACCEPTANCE_PROFILE` is a production-like profile such as:

```text
production
host-prod
single-gpu-prod
multi-gpu-prod
remote-model-prod
```

and `VERITAS_COMMAND_RUNNER` is not explicitly set, Veritas uses the Docker sandbox runner.

Local execution is allowed only when either:

```bash
VERITAS_PROFILE=development
```

or when an operator explicitly opts into local execution:

```bash
VERITAS_ALLOW_LOCAL_COMMAND_RUNNER=true
```

This prevents generated code from running inside the API container or host shell by default.

## Source/mocked proof

Run:

```bash
scripts/e2e/source-mocked-execution-safety.sh
```

The proof checks:

- safe generated writes inside the workspace,
- rejection of `..`, absolute paths, and symlink parent escapes,
- allowlisted compile/test commands,
- rejection of `curl`, `sudo`, `docker`, shell chaining, and inline code execution,
- production profile sandbox default,
- duplicate lock rejection,
- stale-lock replacement,
- state sequence persistence,
- `run_index.jsonl` persistence,
- resume decision after crash-like partial artifacts,
- cancellation blocking.

## Host validation

`validate-host.sh --profile source-mocked` now runs both:

```bash
scripts/e2e/source-mocked-control-plane-e2e.sh
scripts/e2e/source-mocked-execution-safety.sh
```

Docker execution itself remains a live-host validation item. Source/mocked acceptance proves the safety policy and artifact behavior; Docker acceptance proves the actual sandbox runtime.
