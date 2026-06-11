from __future__ import annotations

"""Human-in-the-loop review helpers for Veritas ingestion artifacts."""

import json
from pathlib import Path
from typing import Any, Iterable


def load_chunks_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def write_chunks_jsonl(path: Path, chunks: list[dict[str, Any]]) -> None:
    path.write_text("\n".join(json.dumps(c, ensure_ascii=False) for c in chunks), encoding="utf-8")


def iter_formulas(chunks: list[dict[str, Any]]) -> Iterable[tuple[dict[str, Any], dict[str, Any]]]:
    for chunk in chunks:
        for formula in chunk.get("formulas", []) or []:
            yield chunk, formula


def apply_formula_decision(formula: dict[str, Any], decision: str, corrected_latex: str | None = None, reviewer: str = "human") -> dict[str, Any]:
    """Apply approve/edit/reject/skip decision to a formula object."""

    normalized = decision.strip().lower()
    if normalized not in {"approve", "edit", "reject", "skip", "auto_approve"}:
        raise ValueError(f"Unsupported formula review decision: {decision}")
    formula["human_validation_status"] = normalized
    formula["human_validated"] = normalized in {"approve", "edit", "auto_approve"}
    formula["human_reviewer"] = reviewer
    if normalized == "edit" and corrected_latex:
        formula["original_latex"] = formula.get("latex", "")
        formula["latex"] = corrected_latex.strip()
        formula["normalized_latex"] = " ".join(corrected_latex.strip().split())
    if normalized == "reject":
        formula["use_for_codegen"] = False
    else:
        formula.setdefault("use_for_codegen", True)
    return formula


def review_formulas_noninteractive(path: Path, decision: str, reviewer: str = "human", output: Path | None = None) -> dict[str, Any]:
    chunks = load_chunks_jsonl(path)
    count = 0
    for _chunk, formula in iter_formulas(chunks):
        apply_formula_decision(formula, decision, reviewer=reviewer)
        count += 1
    target = output or path
    write_chunks_jsonl(target, chunks)
    return {"ok": True, "formulas_reviewed": count, "decision": decision, "path": str(target)}
