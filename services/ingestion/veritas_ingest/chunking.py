from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import re
from typing import Any

from .formulas import extract_formulas

_SENTENCE_BOUNDARY_RE = re.compile(r"[.;]")
_WORD_RE = re.compile(r"\S+")


@dataclass(frozen=True)
class Chunk:
    chunk_id: str
    paper_id: str
    ordinal: int
    text: str
    formulas: list[dict[str, Any]]
    metadata: dict[str, Any]
    chunk_type: str = "prose"
    boundary_status: str = "semantic_period_or_semicolon"


def _formula_containing(position: int, formulas: list[dict[str, Any]]) -> dict[str, Any] | None:
    for formula in formulas:
        if formula["start"] <= position < formula["end"]:
            return formula
    return None


def _next_sentence_boundary(text: str, start: int, minimum_end: int, hard_end: int) -> tuple[int, str]:
    """Return a boundary at or after minimum_end, preferring period/semicolon.

    Veritas uses small, precise prose chunks: 25 words, then extend to the next
    period or semicolon. If punctuation is absent before the hard limit, use a
    hard boundary and label it for auditability.
    """
    if minimum_end >= len(text):
        return len(text), "document_end"
    window = text[minimum_end:hard_end]
    match = _SENTENCE_BOUNDARY_RE.search(window)
    if match:
        return minimum_end + match.end(), "semantic_period_or_semicolon"
    return min(hard_end, len(text)), "hard_limit_no_period_or_semicolon"


def _word_boundary_after_n_words(text: str, start: int, word_count: int) -> int:
    if word_count <= 0:
        raise ValueError("word_count must be positive")
    matches = list(_WORD_RE.finditer(text[start:]))
    if not matches:
        return len(text)
    if len(matches) <= word_count:
        return len(text)
    return start + matches[word_count - 1].end()


def _deterministic_formula_id(paper_id: str, formula: dict[str, Any]) -> str:
    key = f"{paper_id}:{formula.get('start')}:{formula.get('end')}:{formula.get('latex','')}"
    return hashlib.sha256(key.encode('utf-8')).hexdigest()[:16]


def _enrich_formula(paper_id: str, formula: dict[str, Any], chunk_id: str) -> dict[str, Any]:
    enriched = dict(formula)
    enriched.setdefault("formula_id", f"{paper_id}::formula::{_deterministic_formula_id(paper_id, formula)}")
    enriched.setdefault("chunk_id", chunk_id)
    enriched.setdefault("latex", str(formula.get("latex", "")).strip())
    enriched.setdefault("normalized_latex", re.sub(r"\s+", " ", enriched["latex"]).strip())
    enriched.setdefault("description", "")
    enriched.setdefault("human_validated", False)
    enriched.setdefault("formula_image_path", "")
    return enriched


def make_chunks(
    paper_id: str,
    text: str,
    metadata: dict[str, Any],
    target_chars: int = 25,
    overlap_chars: int = 0,
    hard_max_chars: int = 1200,
    context_window: int = 650,
) -> list[dict[str, Any]]:
    """Create small prose chunks and first-class formula chunks.

    Backward-compatible parameter names are preserved, but target_chars is now
    interpreted as a word target when it is small (<=100). The production policy
    is 25 words, extended to the nearest following period or semicolon. Formula
    spans are never split; formulas are attached to prose chunks and emitted as
    separate formula chunks so both prose and formulas receive embeddings.

    Acceptance criteria:
        1. Prose chunks target 25 words and end at period/semicolon when possible.
        2. Formula spans are never split.
        3. Formula metadata is structured and deterministic.
        4. Chunks are deterministic and idempotent for the same input.
    """
    if not text or not text.strip():
        return []
    formulas = extract_formulas(text, context_window=context_window)
    chunks: list[Chunk] = []
    ordinal = 0
    start = 0
    text_len = len(text)
    target_words = int(metadata.get("chunk_target_words") or (target_chars if target_chars <= 100 else 25) or 25)
    hard_limit = int(metadata.get("chunk_hard_max_chars") or hard_max_chars or 1200)

    while start < text_len:
        # Avoid starting in the middle of a formula.
        containing = _formula_containing(start, formulas)
        if containing:
            start = containing["end"]
            continue

        min_end = _word_boundary_after_n_words(text, start, target_words)
        hard_end = min(text_len, max(min_end, min(start + hard_limit, text_len)))
        end, boundary_status = _next_sentence_boundary(text, start, min_end, hard_end)

        # Expand through any formula that begins in the chunk and crosses end.
        for formula in formulas:
            if start <= formula["start"] < end and formula["end"] > end:
                end = min(text_len, formula["end"] + context_window)
                boundary_status = "expanded_to_preserve_formula"

        # Do not let hard limits cut a formula. If hard limit lands inside one,
        # place boundary before it if possible, otherwise preserve whole formula.
        if end > start + hard_limit:
            hard = start + hard_limit
            split_formula = _formula_containing(hard, formulas)
            if split_formula and split_formula["start"] > start:
                end = split_formula["start"]
                boundary_status = "hard_limit_before_formula"
            elif split_formula:
                end = min(text_len, split_formula["end"] + context_window)
                boundary_status = "single_formula_exceeds_hard_limit_preserved"
            else:
                end = min(text_len, hard)
                boundary_status = "hard_limit"

        if end <= start:
            break

        ctext = text[start:end].strip()
        if ctext:
            chunk_id = f"{paper_id}::chunk::{ordinal:05d}"
            cformulas = [
                _enrich_formula(paper_id, f, chunk_id)
                for f in formulas
                if start <= f["start"] and f["end"] <= end
            ]
            chunks.append(Chunk(chunk_id, paper_id, ordinal, ctext, cformulas, metadata, "prose", boundary_status))
            ordinal += 1

        start = end
        while start < text_len and text[start].isspace():
            start += 1

    # Also emit each formula as an independent searchable object/chunk.
    for formula in formulas:
        latex = str(formula.get("latex", "")).strip()
        if not latex:
            continue
        formula_chunk_id = f"{paper_id}::formula_chunk::{ordinal:05d}"
        enriched = _enrich_formula(paper_id, formula, formula_chunk_id)
        ftext = "\n".join(part for part in [
            f"Formula: {latex}",
            f"Description: {enriched.get('description','')}",
            f"Context before: {formula.get('context_before','')}",
            f"Context after: {formula.get('context_after','')}",
        ] if part.strip())
        chunks.append(Chunk(formula_chunk_id, paper_id, ordinal, ftext, [enriched], metadata, "formula", "formula_preserved_whole"))
        ordinal += 1

    return [asdict(c) for c in chunks]
