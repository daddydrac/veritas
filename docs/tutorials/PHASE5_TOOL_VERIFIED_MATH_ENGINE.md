# Phase 5 — Tool-Verified Math Engine

Phase 5 adds a real math execution service to Veritas. The LLM may propose, translate, or explain math, but Veritas now checks mathematical claims with executable tools before code generation.

## Real product behavior

For math-heavy runs, the API now attempts to create `math_validation_report.json` before pre-codegen gates run. The report is produced by the `math-tools` service, which executes SymPy/NumPy/mpmath-backed operations such as parsing, normalization, simplification, numeric validation, counterexample search, and property-test generation.

If the math-tools service is unavailable or if any blocking tool returns a failure, the existing pre-codegen Gate Engine returns `blocked_by_math_tools`; generated files are not written and validation commands are not run.

## Tool service

```bash
uvicorn services.math_tools.app:app --host 0.0.0.0 --port 8091
```

Health:

```bash
curl http://localhost:8091/health
```

Formula validation:

```bash
curl -X POST http://localhost:8091/validate \
  -H 'content-type: application/json' \
  -d '{"latex":"E = m c^2","metadata":{"tool_sequence":"parse_latex,normalize_expression,numeric_validate,counterexample_search"}}'
```

## API integration

```bash
curl http://localhost:8080/math-tools/status
```

```bash
curl -X POST http://localhost:8080/math-tools/validate \
  -H 'content-type: application/json' \
  -d '{"formula_latex":"E = m c^2","goal":"validate energy relation"}'
```

## Required artifacts

A math-heavy production-bound run must have:

```text
math_tool_calls.jsonl
math_tool_results.jsonl
math_validation_report.json
```

The pre-codegen gate reads `math_validation_report.json` and blocks if it has counterexamples or blocking findings.

## vLLM tool-calling boundary

The provider client now supports OpenAI-compatible `tools`, `tool_choice`, and `parallel_tool_calls` payload fields. Veritas still owns actual execution: tool calls are validated and executed by the application, not trusted as model claims.
