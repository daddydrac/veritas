from __future__ import annotations

"""Formula image extraction and OCR metadata attachment.

Phase 6 makes formula images and OCR auditable without making a heavy OCR
model mandatory.  The production renderer uses PyMuPDF when it is installed and
Docling supplies page/bbox coordinates.  CI and source/mocked E2E use the
``VERITAS_FORMULA_IMAGE_RENDERER=mock`` renderer, which writes a deterministic
1x1 PNG so the downstream OCR/review contract can be tested without Docker,
Cargo, GPUs, or a real PDF renderer.
"""

from pathlib import Path
from typing import Any
import base64
import hashlib
import os

from .latex_ocr import normalize_latex, ocr_formula_image

_ONE_PIXEL_PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
)


def attach_formula_images(pdf_path: Path, chunks: list[dict[str, Any]], output_root: Path) -> list[dict[str, Any]]:
    """Attach image/OCR/review metadata to formula objects.

    Acceptance criteria:
        1. Preserve formulas even when image extraction fails.
        2. Use explicit status fields for every fallback path.
        3. Support real PyMuPDF rendering when page+bbox metadata exists.
        4. Support deterministic mock rendering for CI/source-level proofs.
        5. Attach normalized LaTeX, OCR status, engine, confidence, and review
           defaults for OpenSearch/Fuseki persistence.
    """

    output_root.mkdir(parents=True, exist_ok=True)
    renderer = os.getenv("VERITAS_FORMULA_IMAGE_RENDERER", "auto").strip().lower() or "auto"

    fitz = None
    doc = None
    if renderer != "mock":
        try:
            import fitz as _fitz  # type: ignore
            fitz = _fitz
        except Exception:
            fitz = None
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
            _ensure_formula_defaults(formula)
            page = formula.get("page")
            bbox = formula.get("bbox")
            formula_id = str(formula.get("formula_id") or _formula_hash(formula))
            formula["formula_id"] = formula_id

            if renderer == "mock":
                image_path = doc_dir / f"{_safe_name(formula_id)}.png"
                image_path.write_bytes(_ONE_PIXEL_PNG)
                formula["formula_image_path"] = str(image_path)
                formula["formula_image_status"] = "rendered_mock"
                formula["formula_image_engine"] = "mock"
                formula["formula_image_confidence"] = 0.60
                formula["bbox_status"] = "bbox_present" if bbox else "bbox_missing_mock_allowed"
                _apply_latex_ocr(formula, image_path)
                continue

            if doc is None:
                formula["formula_image_status"] = "not_available_renderer_unavailable" if bbox else "not_available_no_bbox_or_renderer"
                formula["formula_image_engine"] = "none"
                formula["formula_image_confidence"] = 0.0
                formula["bbox_status"] = "bbox_present" if bbox else "bbox_missing"
                _apply_latex_ocr(formula, Path(formula.get("formula_image_path") or ""))
                continue

            if page is None or not bbox:
                formula["formula_image_status"] = "not_available_no_bbox"
                formula["formula_image_engine"] = "pymupdf"
                formula["formula_image_confidence"] = 0.0
                formula["bbox_status"] = "bbox_missing"
                _apply_latex_ocr(formula, Path(formula.get("formula_image_path") or ""))
                continue

            try:
                page_index = max(0, int(page) - 1)
                rect = fitz.Rect([float(x) for x in bbox])
                pix = doc[page_index].get_pixmap(clip=rect, dpi=int(os.getenv("VERITAS_FORMULA_IMAGE_DPI", "200")))
                image_path = doc_dir / f"{_safe_name(formula_id)}.png"
                pix.save(str(image_path))
                formula["formula_image_path"] = str(image_path)
                formula["formula_image_status"] = "rendered"
                formula["formula_image_engine"] = "pymupdf"
                formula["formula_image_confidence"] = float(formula.get("confidence") or 0.82)
                formula["bbox_status"] = "bbox_present"
                _apply_latex_ocr(formula, image_path)
            except Exception as exc:  # keep ingestion recoverable
                formula["formula_image_status"] = "render_failed"
                formula["formula_image_engine"] = "pymupdf"
                formula["formula_image_confidence"] = 0.0
                formula["formula_image_message"] = str(exc)[:500]
                formula["bbox_status"] = "bbox_present"
                _apply_latex_ocr(formula, Path(formula.get("formula_image_path") or ""))
    if doc is not None:
        doc.close()
    return chunks


def _ensure_formula_defaults(formula: dict[str, Any]) -> None:
    formula.setdefault("formula_image_path", "")
    formula.setdefault("formula_image_status", "not_attempted")
    formula.setdefault("formula_image_engine", "none")
    formula.setdefault("formula_image_confidence", 0.0)
    formula.setdefault("bbox_status", "unknown")
    formula.setdefault("latex_ocr_status", "not_attempted")
    formula.setdefault("latex_ocr_engine", "none")
    formula.setdefault("latex_ocr_confidence", 0.0)
    formula.setdefault("human_validated", False)
    formula.setdefault("human_validation_status", "pending_human_review")
    formula.setdefault("use_for_codegen", False)
    formula.setdefault("description", _heuristic_formula_description(str(formula.get("latex", ""))))
    if formula.get("latex"):
        formula.setdefault("normalized_latex", normalize_latex(str(formula.get("latex", ""))))


def _formula_hash(formula: dict[str, Any]) -> str:
    seed = f"{formula.get('latex','')}:{formula.get('start','')}:{formula.get('end','')}:{formula.get('page','')}:{formula.get('bbox','')}"
    return hashlib.sha256(seed.encode("utf-8")).hexdigest()[:16]


def _safe_name(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in {"-", "_"} else "_" for ch in value).strip("_") or "item"


def _heuristic_formula_description(latex: str) -> str:
    latex = latex.strip()
    if not latex:
        return ""
    if "\\mathbb{E}" in latex or "E_" in latex or "\\operatorname{E}" in latex:
        return "Expectation-style symbolic shadow; inspect distribution, variables, and invariant obligations before implementation."
    if "\\sum" in latex:
        return "Summation symbolic shadow; inspect index bounds, convergence, and numerical stability."
    if "\\int" in latex:
        return "Integral symbolic shadow; inspect domain, measure, approximation method, and error tolerance."
    if "\\nabla" in latex or "grad" in latex.lower():
        return "Gradient or differential symbolic shadow; inspect smoothness assumptions and stability."
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
