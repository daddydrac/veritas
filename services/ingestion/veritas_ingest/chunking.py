from __future__ import annotations
from dataclasses import dataclass, asdict
from .formulas import extract_formulas

@dataclass
class Chunk:
    chunk_id: str
    paper_id: str
    ordinal: int
    text: str
    formulas: list[dict]
    metadata: dict


def _formula_containing(position: int, formulas: list[dict]) -> dict | None:
    for formula in formulas:
        if formula["start"] < position < formula["end"]:
            return formula
    return None


def make_chunks(paper_id: str, text: str, metadata: dict, target_chars: int, overlap_chars: int, hard_max_chars: int, context_window: int) -> list[dict]:
    formulas = extract_formulas(text, context_window=context_window)
    chunks: list[Chunk] = []
    start = 0
    ordinal = 0
    text_len = len(text)

    if target_chars <= 0 or hard_max_chars <= 0:
        raise ValueError("target_chars and hard_max_chars must be positive")
    if overlap_chars < 0:
        raise ValueError("overlap_chars must be non-negative")

    while start < text_len:
        # If overlap lands inside a formula, skip to the end of that formula.
        # The full formula should already belong to the previous chunk; do not
        # emit a chunk that starts with partial LaTeX.
        containing_start = _formula_containing(start, formulas)
        if containing_start is not None:
            start = containing_start["end"]
            if start >= text_len:
                break

        end = min(text_len, start + target_chars)
        boundary = max(text.rfind("\n\n", start, end), text.rfind(". ", start, end))
        if boundary > start + int(target_chars * 0.55):
            end = boundary + 1

        # Expand end if the boundary falls inside a formula or if a formula starts
        # in this chunk and extends past the proposed boundary.
        containing_end = _formula_containing(end, formulas)
        if containing_end is not None:
            end = min(text_len, containing_end["end"] + context_window)
        for formula in formulas:
            if start <= formula["start"] < end and formula["end"] > end:
                end = min(text_len, formula["end"] + context_window)

        # Respect hard max, but never clip through a formula. If hard-max would
        # split one, end before it and let the next chunk handle it.
        hard_end = min(start + hard_max_chars, text_len)
        if end > hard_end:
            split_formula = _formula_containing(hard_end, formulas)
            if split_formula is not None and split_formula["start"] > start:
                end = split_formula["start"]
            elif split_formula is not None:
                # A single formula is longer than the configured hard limit. Preserve
                # the full formula instead of creating mathematically corrupt chunks.
                end = min(text_len, split_formula["end"] + context_window)
            else:
                end = hard_end

        if end <= start:
            end = min(text_len, start + target_chars)

        ctext = text[start:end].strip()
        if ctext:
            cformulas = [f for f in formulas if start <= f["start"] and f["end"] <= end]
            chunks.append(Chunk(f"{paper_id}::chunk::{ordinal:05d}", paper_id, ordinal, ctext, cformulas, metadata))
            ordinal += 1

        if end >= text_len:
            break
        next_start = max(0, end - overlap_chars)
        if next_start <= start:
            next_start = end
        start = next_start

    return [asdict(c) for c in chunks]
