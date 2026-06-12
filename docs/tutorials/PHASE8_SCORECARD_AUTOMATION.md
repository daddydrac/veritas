# Phase 8 — Documentation and Metric Automation

Phase 8 makes Veritas self-report its source/mocked readiness score without claiming live-host validation that has not been executed.

## Scope

Phase 8 keeps the agreed source/mocked scope:

- Cargo/Rust validation is `host_validation_pending`.
- Docker Compose E2E validation is `host_validation_pending`.
- Live vLLM/GPU validation is `host_validation_pending`.

All other production-readiness features are scored through source files, schemas, source/mocked E2E scripts, generated evidence, and validation outputs.

## Commands

Generate the feature scorecard and update documentation:

```bash
scripts/e2e/source-mocked-scorecard.sh
```

Or run directly:

```bash
python3 scripts/generate-feature-scorecard.py --run-validate-spec --update-docs
```

Outputs:

```text
data/scorecard/feature-scorecard.json
FEATURE_SCORECARD.md
VALIDATION_MATRIX.md
AUDIT.md
FEATURES.md
```

## Acceptance criteria

The source/mocked scorecard is accepted when:

- `scripts/validate-spec.py` has zero failed checks.
- All non-skipped source/mocked features have A/B grades.
- The source/mocked average score is at least 94%.
- Rust/Cargo, Docker E2E, and live vLLM/GPU are marked `host_validation_pending`, not failed.
- README, QUICKSTART, FEATURES, AUDIT, and VALIDATION_MATRIX do not claim live host validation unless it was actually run.
