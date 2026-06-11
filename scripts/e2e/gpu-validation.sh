#!/usr/bin/env bash
set -euo pipefail
# Validates VERITAS_PLANNER_TENSOR_PARALLEL_SIZE, VERITAS_CODE_TENSOR_PARALLEL_SIZE, and VERITAS_MATH_TENSOR_PARALLEL_SIZE.
if [ ! -f .veritas/runtime.env ]; then
  echo "Missing .veritas/runtime.env. Run veritas init first." >&2
  exit 1
fi
if command -v cargo >/dev/null 2>&1; then
  cargo run -p veritas -- gpu-validate
else
  echo "cargo unavailable; falling back to shell validation of GPU layout."
  source .veritas/runtime.env
  : "${VERITAS_GPU_COUNT:=0}"
  for role in PLANNER CODE MATH; do
    ids_var="VERITAS_${role}_GPU_DEVICE_IDS"
    tp_var="VERITAS_${role}_TENSOR_PARALLEL_SIZE"
    ids="${!ids_var:-0}"
    tp="${!tp_var:-1}"
    count=$(python3 - <<PY
print(len([x for x in '$ids'.split(',') if x.strip()]))
PY
)
    if [ "$VERITAS_GPU_COUNT" != "0" ] && [ "$tp" -gt "$count" ]; then
      echo "$role tensor_parallel_size=$tp exceeds assigned GPU count=$count ($ids)" >&2
      exit 1
    fi
  done
  echo "Shell GPU layout validation passed."
fi
