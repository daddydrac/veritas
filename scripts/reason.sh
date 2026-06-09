#!/usr/bin/env bash
set -euo pipefail
docker compose run --rm reasoner consistency /workspace/ontology/veritas.owl
