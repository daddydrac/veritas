# Tutorial 001 — Bootstrap Veritas

## Goal
Run the local Veritas stack with OpenSearch, Fuseki, API, and ingestion tooling.

## Steps

```bash
cp .env.example .env
./scripts/bootstrap.sh
curl http://localhost:8080/health
```

## Acceptance Criteria

- API health returns `ok`.
- OpenSearch is reachable on port 9200.
- Fuseki is reachable on port 3030.
