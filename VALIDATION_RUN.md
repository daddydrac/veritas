# Validation Run

Generated during the repository update.

## Commands executed in this environment

```bash
python3 -m compileall services/embedding services/ingestion/veritas_ingest
```

Result: passed.

```bash
python3 - <<'PY'
import yaml
for f in ['docker-compose.yml', 'config/veritas.yaml']:
    yaml.safe_load(open(f))
PY
```

Result: passed.

```bash
PYTHONPATH=services/ingestion pytest -q tests/ingestion
```

Result: 9 passed.

```bash
PYTHONPATH=services/ingestion python scripts/validate-spec.py
```

Result: ok=true, failed=0, unavailable=2.

## Commands not executed here

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
docker compose config
./scripts/bootstrap.sh
docker compose --profile models --profile code-model --profile math-model up -d
docker compose run --rm cli ingest-pdf --path tests/fixtures/sample_math_paper.pdf
docker compose run --rm cli run "Implement the indexed formula as a tested Rust package" --language rust
```

Reason: this sandbox does not provide Cargo, Docker, or GPU runtime.
