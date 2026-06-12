#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
export PYTEST_DISABLE_PLUGIN_AUTOLOAD=1
export PYTHONPATH="services/ingestion:${PYTHONPATH:-}"

profile=${VERITAS_ACCEPTANCE_PROFILE:-host-prod}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) profile=${2:?--profile requires a value}; shift 2 ;;
    --skip-cargo) export VERITAS_SKIP_CARGO_VALIDATION=true; shift ;;
    --skip-docker) export VERITAS_SKIP_DOCKER_VALIDATION=true; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

export VERITAS_ACCEPTANCE_PROFILE="$profile"
case "$profile" in
  source-mocked|fake-ci)
    export VERITAS_ACCEPTANCE_MODE=mocked_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=${VERITAS_SKIP_CARGO_VALIDATION:-true}
    export VERITAS_SKIP_DOCKER_VALIDATION=${VERITAS_SKIP_DOCKER_VALIDATION:-true}
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}
    ;;
  single-gpu-prod|multi-gpu-prod|live-gpu)
    export VERITAS_ACCEPTANCE_MODE=live_gpu_acceptance
    export VERITAS_SKIP_CARGO_VALIDATION=${VERITAS_SKIP_CARGO_VALIDATION:-false}
    export VERITAS_SKIP_DOCKER_VALIDATION=${VERITAS_SKIP_DOCKER_VALIDATION:-false}
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-true}
    ;;
  *)
    export VERITAS_ACCEPTANCE_MODE=${VERITAS_ACCEPTANCE_MODE:-host_acceptance}
    export VERITAS_SKIP_CARGO_VALIDATION=${VERITAS_SKIP_CARGO_VALIDATION:-false}
    export VERITAS_SKIP_DOCKER_VALIDATION=${VERITAS_SKIP_DOCKER_VALIDATION:-false}
    export VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}
    ;;
esac

mkdir -p data/e2e
results_file=data/e2e/host-validation-steps.jsonl
: > "$results_file"
python3 -c 'import json, pathlib, time; pathlib.Path("data/e2e/host-validation-start.json").write_text(json.dumps({"started_at_epoch": int(time.time())}, indent=2))'

record_step() {
  local name="$1" status="$2" details="${3:-}" epoch
  epoch=$(date +%s)
  name=${name//\\/\\\\}; name=${name//\"/\\\"}
  status=${status//\\/\\\\}; status=${status//\"/\\\"}
  details=${details//\\/\\\\}; details=${details//\"/\\\"}
  printf '{"name":"%s","status":"%s","details":"%s","epoch":%s}\n' "$name" "$status" "$details" "$epoch" >> "$results_file"
}

run_step() {
  local name="$1"; shift
  echo "== $name =="
  "$@"
  record_step "$name" passed
}
skip_step() {
  local name="$1" reason="$2"
  echo "== $name skipped: $reason =="
  record_step "$name" skipped "$reason"
}

run_step "Packaging check" scripts/check-packaging.sh
run_step "Python compile" python3 -m compileall services/embedding services/ingestion services/shacl tests/fakes
run_step "Real local ingestion backend" pytest -q tests/ingestion/test_phase2_real_local_ingestion_backend.py --disable-warnings
if [ "${VERITAS_ACCEPTANCE_MODE:-host_acceptance}" = "mocked_acceptance" ]; then
  skip_step "Python phase tests" "source-mocked profile runs focused E2E; run PYTHONPATH=services/ingestion pytest -q tests/ingestion separately for full Python coverage"
else
  run_step "Python tests" pytest -q tests/ingestion --disable-warnings -k "not phase2_source_mocked_control_plane_e2e_runs_and_validates"
fi
run_step "Source-mocked control-plane E2E" scripts/e2e/source-mocked-control-plane-e2e.sh
run_step "Source-mocked execution safety" scripts/e2e/source-mocked-execution-safety.sh
run_step "Source-mocked retrieval ontology" scripts/e2e/source-mocked-retrieval-ontology.sh
run_step "Source-mocked SHACL governance" scripts/e2e/source-mocked-shacl-governance.sh
run_step "Source-mocked formula OCR review" scripts/e2e/source-mocked-formula-ocr-review.sh
run_step "Source-mocked human workflow" scripts/e2e/source-mocked-human-workflow.sh
run_step "Source-mocked scorecard" scripts/e2e/source-mocked-scorecard.sh

if [ "${VERITAS_SKIP_CARGO_VALIDATION:-false}" = "true" ]; then
  skip_step "cargo fmt/check/test/clippy" "skipped by profile $profile"
else
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required for this validation profile but is unavailable." >&2
    exit 2
  fi
  run_step "cargo fmt" cargo fmt --all -- --check
  run_step "cargo check" cargo check --workspace
  run_step "cargo test" cargo test --workspace
  run_step "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
fi

if [ "${VERITAS_SKIP_DOCKER_VALIDATION:-false}" = "true" ]; then
  skip_step "docker compose config and fake-vLLM E2E" "skipped by profile $profile"
else
  if ! command -v docker >/dev/null 2>&1; then
    echo "Docker is required for this validation profile but is unavailable." >&2
    exit 2
  fi
  if [ ! -f .veritas/runtime.env ]; then
    scripts/e2e/write-fake-runtime-env.sh
  fi
  run_step "docker compose config" docker compose --env-file .veritas/runtime.env config
  run_step "GPU layout validation" scripts/e2e/gpu-validation.sh
  run_step "fake-vLLM Docker E2E" scripts/e2e/full-fake-vllm-e2e.sh
fi

if [ "${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}" = "true" ]; then
  if [ "${VERITAS_SKIP_DOCKER_VALIDATION:-false}" = "true" ]; then
    echo "live vLLM validation cannot run while Docker validation is skipped." >&2
    exit 2
  fi
  run_step "live vLLM model smoke" scripts/e2e/live-vllm-smoke.sh
else
  skip_step "live vLLM model smoke" "set VERITAS_REQUIRE_LIVE_VLLM_VALIDATION=true or use --profile single-gpu-prod"
fi

if [ -f validation-last.json ]; then
  record_step "Validate spec summary" passed "validation-last.json present"
else
  record_step "Validate spec summary" skipped "validation-last.json missing"
fi
if [ "${VERITAS_UPDATE_AUDIT_DURING_HOST_VALIDATE:-false}" = "true" ]; then
  python3 scripts/update-audit.py
else
  echo "AUDIT.md update skipped during host validation; run python3 scripts/update-audit.py when desired."
fi
total_steps=$(wc -l < "$results_file" | tr -d ' ')
passed_steps=$(grep -c '"status":"passed"' "$results_file" || true)
skipped_steps=$(grep -c '"status":"skipped"' "$results_file" || true)
failed_steps=$(grep -c '"status":"failed"' "$results_file" || true)
cat > data/e2e/host-validation-summary.json <<EOF
{
  "ok": true,
  "profile": "$VERITAS_ACCEPTANCE_PROFILE",
  "acceptance_mode": "$VERITAS_ACCEPTANCE_MODE",
  "cargo_validation": "$( [ "${VERITAS_SKIP_CARGO_VALIDATION:-false}" = "true" ] && echo skipped || echo passed )",
  "docker_validation": "$( [ "${VERITAS_SKIP_DOCKER_VALIDATION:-false}" = "true" ] && echo skipped || echo passed )",
  "live_vllm_required": $( [ "${VERITAS_REQUIRE_LIVE_VLLM_VALIDATION:-false}" = "true" ] && echo true || echo false ),
  "step_counts": {
    "total": $total_steps,
    "passed": $passed_steps,
    "skipped": $skipped_steps,
    "failed": $failed_steps
  }
}
EOF
cat data/e2e/host-validation-summary.json
exit 0
