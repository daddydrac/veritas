# Phase 0 — Packaging, Validation, and Scoring Cleanup

Phase 0 removes release-packaging friction and separates mocked/source acceptance from live host acceptance.

## Business outcome

A contributor or evaluator can unzip Veritas, run source-level validation, and receive a structured pass/fail report without manually repairing file permissions or guessing whether missing Docker/Cargo/vLLM tools should count as failed production behavior.

## Commands

```bash
scripts/check-packaging.sh
scripts/production-acceptance.sh --profile source-mocked
```

On a live GPU host, use:

```bash
scripts/production-acceptance.sh --profile single-gpu-prod
```

## Profiles

- `source-mocked`: validates source/mocked acceptance and skips Cargo, Docker, and live vLLM checks.
- `fake-ci`: same practical behavior as `source-mocked`, intended for CI.
- `host-prod`: requires Cargo and Docker but does not require live vLLM unless explicitly requested.
- `single-gpu-prod` and `multi-gpu-prod`: require Cargo, Docker, GPU validation, fake-vLLM E2E, and live vLLM smoke validation.
- `remote-model-prod`: validates host readiness while allowing remote model serving policy.

## Acceptance criteria

- Every `scripts/**/*.sh` file is executable.
- `.github/workflows/python.yml`, `.github/workflows/rust.yml`, and `.github/workflows/docker-e2e.yml` exist.
- `scripts/validate-spec.py` emits structured JSON and does not crash on optional-file checks.
- `VALIDATION_MATRIX.md` distinguishes source/mocked acceptance from live host acceptance.
- Live host validation remains documented and cannot be claimed until actually run.
