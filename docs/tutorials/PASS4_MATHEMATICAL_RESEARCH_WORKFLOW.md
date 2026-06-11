# Pass 4 — Mathematical Research Workflow

Pass 4 hardens Veritas as a math-heavy research-to-code system rather than a generic coding agent.

## Implemented source-level changes

- Formula extraction now merges regex/Markdown formulas with Docling visual formula candidates.
- Formula candidates preserve page, bounding box, extraction source, confidence, image path, OCR status, and human validation status.
- Formula images are rasterized when PyMuPDF and page/bbox metadata are available.
- LaTeX OCR is pluggable through `VERITAS_LATEX_OCR_PROVIDER`:
  - `heuristic` for deterministic CI and preserving existing LaTeX,
  - `command` for a local OCR executable,
  - `http` for an OCR service,
  - `none` to disable OCR explicitly.
- `review-formulas` supports human approval/edit/rejection over extracted formula chunks.
- `/math-to-code` now performs representation-first math reasoning before code generation.
- `/math-to-code` can return `awaiting_human_checkpoint` when policy requires human review.
- `/human/checkpoint` stores human decisions in the run workspace.
- `schemas/math_reasoning.schema.json` now mirrors the MATH.md reasoning skeleton.
- `packages/ontology/shacl/veritas-math.shacl.ttl` adds representation-first SHACL readiness checks.

## Representation-first contract

Veritas treats formulas as `SymbolicShadow` artifacts. A formula is not accepted as truth by itself. The math model must produce:

```text
surface phenomenon
representation hypothesis
candidate representation map
primitive ontology
transformation space
constraint geometry
invariants
compression fidelity
recursive closure
generative necessity
symbolic shadows
transfer tests
risks
validation requirements
status
```

## Human checkpoint flow

```text
veritas math-to-code --formula-latex 'L(\theta)=...'
  → API runs math reasoning schema
  → API returns checkpoint if policy requires review
  → CLI displays representation/invariant/risk summary
  → human approves or rejects
  → approved request triggers code generation and validation
```

Formula review after ingestion:

```bash
veritas review-formulas --chunks data/chunks/<paper>.chunks.jsonl
```

CI/non-interactive approval:

```bash
python -m veritas_ingest.cli review-formulas \
  --chunks data/chunks/<paper>.chunks.jsonl \
  --decision approve \
  --reviewer ci
```

## Remaining host validation

Cargo and Docker are required to validate the Rust API/CLI, live vLLM, live OpenSearch, live Fuseki, live SHACL, and sandboxed command execution.
