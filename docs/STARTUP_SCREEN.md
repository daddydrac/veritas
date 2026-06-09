# Veritas CLI Startup Screen

The CLI startup screen is implemented in `apps/cli/src/main.rs`.

Run it with:

```bash
veritas
# or
docker compose run --rm cli welcome
```

It prints:

- Veritas ASCII logo
- `Mathematical Truth Through Evidence`
- product tagline
- live service status from `GET /ready`
- live knowledge-graph counts from `GET /graph/status`
- ontology, reasoner, graph, and vector-memory status
- guided non-coder workflow menu
- research, engineering, operations, and autonomous-mode guidance

The API endpoint `GET /graph/status` queries Fuseki with SPARQL count queries for:

- `Objective`
- `Plan`
- `TaskSpecification`
- `Risk`
- `Invariant`
- `EvidenceArtifact`
- `ValidationCheckSpecification`

If the API, Fuseki, or graph is not reachable, the CLI prints `unknown` statuses and continues with remediation-friendly guidance instead of crashing.
