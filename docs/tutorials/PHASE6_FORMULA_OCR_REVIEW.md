# Phase 6 — Formula OCR and Formula Review

Phase 6 hardens the formula-extraction, image/OCR, citation-review, and formula-review contract at source/mocked level. Cargo/Rust validation, Docker E2E, and live vLLM/GPU validation remain host-only gates for this scoped pass.

## Business outcome

A research PDF can produce formula objects that are auditable before they are used for code generation. Each formula carries source, image status, OCR status, normalized LaTeX, confidence, description, review status, and codegen eligibility. Citations can be reviewed as APA-style audit anchors so a human can trace generated code back to the source paper.

## New source/mocked proof

```bash
scripts/e2e/source-mocked-formula-ocr-review.sh
```

The proof validates:

1. Command-based LaTeX OCR provider path.
2. HTTP-based LaTeX OCR provider path.
3. Mock formula image renderer for CI/source-level testing.
4. Formula review persistence into chunks and RDF metadata.
5. Citation approve/edit/reject/incomplete persistence.
6. Chunking edge cases around abbreviations, missing punctuation, semicolons, and multiple formulas.
7. OpenSearch mapping fields for OCR/review metadata.

## OCR provider modes

```text
VERITAS_LATEX_OCR_PROVIDER=none      # preserve existing LaTeX only
VERITAS_LATEX_OCR_PROVIDER=heuristic # deterministic fallback for CI
VERITAS_LATEX_OCR_PROVIDER=command   # run local OCR command
VERITAS_LATEX_OCR_PROVIDER=http      # call HTTP OCR service
```

Command provider example:

```bash
export VERITAS_LATEX_OCR_PROVIDER=command
export VERITAS_LATEX_OCR_COMMAND='python /opt/ocr/image_to_latex.py {image}'
```

HTTP provider example:

```bash
export VERITAS_LATEX_OCR_PROVIDER=http
export VERITAS_LATEX_OCR_URL='http://latex-ocr:8000/ocr'
```

Both providers may return either raw LaTeX or JSON:

```json
{"latex":"E = mc^2", "confidence":0.93, "message":"ok"}
```

## Formula image renderer modes

```text
VERITAS_FORMULA_IMAGE_RENDERER=auto # use PyMuPDF when available
VERITAS_FORMULA_IMAGE_RENDERER=mock # deterministic CI/source fixture image
```

The `mock` renderer is not a claim of visual OCR quality. It proves that downstream metadata, OCR provider calls, review state, OpenSearch mapping, and RDF persistence behave correctly without requiring a real PDF renderer.

## Human review commands

Review formulas:

```bash
python -m veritas_ingest.cli review-formulas \
  --chunks data/chunks/paper.chunks.jsonl \
  --decision approve \
  --reviewer alice
```

Review citations:

```bash
python -m veritas_ingest.cli review-citations \
  --chunks data/chunks/paper.chunks.jsonl \
  --decision edit \
  --corrected-citation 'Doe, J. (2026). Corrected paper title.' \
  --reviewer alice
```

Validate formula readiness:

```bash
python -m veritas_ingest.cli validate-formulas \
  --chunks data/chunks/paper.chunks.jsonl
```

## Production boundary

Phase 6 proves the contract with mocked OCR providers. Live formula OCR quality on representative arXiv PDFs must still be validated on the target corpus before claiming corpus-level extraction accuracy.
