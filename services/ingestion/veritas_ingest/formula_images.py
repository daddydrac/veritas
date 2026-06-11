from __future__ import annotations

from pathlib import Path
from typing import Any
import hashlib
import os

from .latex_ocr import normalize_latex, ocr_formula_image


def attach_formula_images(pdf_path: Path, chunks: list[dict[str, Any]], output_root: Path) -> list[dict[str, Any]]:
    """Attach formula image paths when page/bbox metadata is available.

    This implementation is production-safe and dependency-optional:
    - If PyMuPDF is installed and formulas include page+bbox, formula regions are rasterized.
    - If either dependency or bbox metadata is unavailable, formulas are preserved with an
      explicit status so downstream math-to-code knows human review is required.

    The function mutates a copy-like object in place for ingestion efficiency and returns it.
    """

    output_root.mkdir(parents=True, exist_ok=True)
    try:
        import fitz  # type: ignore
    except Exception:
        fitz = None  # noqa: N806

    doc = None
    if fitz is not None and pdf_path.exists():
        try:
            doc = fitz.open(str(pdf_path))
        except Exception:
            doc = None

    for chunk in chunks:
        paper_id = str(chunk.get("paper_id", "paper"))
        doc_dir = output_root / _safe_name(paper_id)
        doc_dir.mkdir(parents=True, exist_ok=True)
        for formula in chunk.get("formulas", []) or []:
            formula.setdefault("formula_image_path", "")
            formula.setdefault("formula_image_status", "not_attempted")
            formula.setdefault("latex_ocr_status", "not_attempted")
            formula.setdefault("human_validated", False)
            formula.setdefault("description", _heuristic_formula_description(str(formula.get("latex", ""))))
            formula.setdefault("human_validation_status", "pending_human_review")
            page = formula.get("page")
            bbox = formula.get("bbox")
            if doc is None or page is None or not bbox:
                formula["formula_image_status"] = "not_available_no_bbox_or_renderer"
                _apply_latex_ocr(formula, Path(formula.get("formula_image_path") or ""))
                continue
            try:
                page_index = max(0, int(page) - 1)
                rect = fitz.Rect([float(x) for x in bbox])
                pix = doc[page_index].get_pixmap(clip=rect, dpi=200)
                formula_id = str(formula.get("formula_id") or _formula_hash(formula))
                image_path = doc_dir / f"{_safe_name(formula_id)}.png"
                pix.save(str(image_path))
                formula["formula_image_path"] = str(image_path)
                formula["formula_image_status"] = "rendered"
                _apply_latex_ocr(formula, image_path)
            except Exception as exc:  # keep ingestion recoverable
                formula["formula_image_status"] = f"render_failed:{exc}"
                _apply_latex_ocr(formula, Path(formula.get("formula_image_path") or ""))
    if doc is not None:
        doc.close()
    return chunks


def _formula_hash(formula: dict[str, Any]) -> str:
    seed = f"{formula.get('latex','')}:{formula.get('start','')}:{formula.get('end','')}"
    return hashlib.sha256(seed.encode("utf-8")).hexdigest()[:16]


def _safe_name(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in {"-", "_"} else "_" for ch in value).strip("_") or "item"


def _heuristic_formula_description(latex: str) -> str:
    latex = latex.strip()
    if not latex:
        return ""
    if "\\mathbb{E}" in latex or "E_" in latex or "\\operatorname{E}" in latex:
        return "Expectation-style expression; inspect variables, distribution, and invariants before implementation."
    if "\\sum" in latex:
        return "Summation expression; inspect index bounds, convergence, and numerical stability."
    if "\\int" in latex:
        return "Integral expression; inspect domain, measure, approximation method, and error tolerance."
    if "\\nabla" in latex or "grad" in latex.lower():
        return "Gradient or differential expression; inspect smoothness assumptions and stability."
    if "=" in latex:
        return "Equation-like symbolic shadow; identify the constrained transformation and preserved invariant."
    return "Formula-like symbolic shadow extracted from the source document."


def _apply_latex_ocr(formula: dict[str, Any], image_path: Path) -> None:
    """Attach OCR result without making OCR a hard ingestion dependency."""

    provider = os.getenv("VERITAS_LATEX_OCR_PROVIDER", "heuristic")
    existing = str(formula.get("latex", ""))
    result = ocr_formula_image(image_path, existing_latex=existing, provider=provider)
    formula["latex_ocr_status"] = result.status
    formula["latex_ocr_engine"] = result.engine
    formula["latex_ocr_confidence"] = result.confidence
    if result.message:
        formula["latex_ocr_message"] = result.message
    if result.latex and (not existing.strip() or provider.lower() in {"command", "http"}):
        formula.setdefault("original_latex", existing)
        formula["latex"] = result.latex
        formula["normalized_latex"] = normalize_latex(result.latex)
    elif existing.strip():
        formula["normalized_latex"] = normalize_latex(existing)
    if not formula.get("description"):
        formula["description"] = _heuristic_formula_description(str(formula.get("latex", "")))
