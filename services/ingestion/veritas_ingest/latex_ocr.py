from __future__ import annotations

"""Formula image to LaTeX OCR providers for Veritas ingestion.

The production contract is intentionally pluggable. Veritas can run without a
bundled OCR model, but it can use a local command or HTTP service when the user
configures one. Tests use the deterministic heuristic/mock provider so CI does
not need a GPU model.
"""

import base64
import json
import os
import re
import shlex
import subprocess
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class LatexOcrResult:
    latex: str
    status: str
    engine: str
    confidence: float
    message: str = ""

    def as_dict(self) -> dict[str, Any]:
        return asdict(self)


_MATH_WORDS = {
    "alpha": "\\alpha",
    "beta": "\\beta",
    "gamma": "\\gamma",
    "theta": "\\theta",
    "lambda": "\\lambda",
    "sigma": "\\sigma",
    "sum": "\\sum",
    "int": "\\int",
    "nabla": "\\nabla",
}


def normalize_latex(latex: str) -> str:
    """Normalize LaTeX for equality/search without changing semantics."""

    text = latex.strip()
    text = text.replace("\u2212", "-")
    text = re.sub(r"\s+", " ", text)
    text = re.sub(r"\s*([=+\-*/_^{}()\[\],])\s*", r"\1", text)
    return text.strip()


def ocr_formula_image(image_path: str | Path, existing_latex: str = "", provider: str | None = None) -> LatexOcrResult:
    """Return a LaTeX OCR result for a formula image.

    Providers:
      - none: never OCR; preserves existing LaTeX if present.
      - heuristic: deterministic fallback, useful for tests and CI.
      - command: call VERITAS_LATEX_OCR_COMMAND with {image} placeholder.
      - http: POST base64 image to VERITAS_LATEX_OCR_URL and parse JSON.

    The function is side-effect free except for the configured external call. It
    never raises for ordinary OCR failure; it returns an auditable status.
    """

    chosen = (provider or os.getenv("VERITAS_LATEX_OCR_PROVIDER") or "heuristic").strip().lower()
    image = Path(image_path)
    existing = normalize_latex(existing_latex or "")

    if chosen in {"", "none", "disabled"}:
        return LatexOcrResult(existing, "skipped_provider_disabled", "none", 1.0 if existing else 0.0)

    if chosen == "heuristic":
        if existing:
            return LatexOcrResult(existing, "skipped_existing_latex", "heuristic", 0.90)
        guessed = _guess_latex_from_filename(image)
        return LatexOcrResult(guessed, "heuristic_guess" if guessed else "ocr_unavailable", "heuristic", 0.35 if guessed else 0.0)

    if chosen == "command":
        return _ocr_with_command(image, existing)

    if chosen == "http":
        return _ocr_with_http(image, existing)

    return LatexOcrResult(existing, "unknown_provider", chosen, 0.0, f"Unknown OCR provider: {chosen}")


def _guess_latex_from_filename(image: Path) -> str:
    stem = image.stem.lower().replace("formula", "").replace("_", " ")
    tokens: list[str] = []
    for raw in re.split(r"[^a-z0-9]+", stem):
        if not raw:
            continue
        tokens.append(_MATH_WORDS.get(raw, raw))
    if any(t.startswith("\\") for t in tokens):
        return " ".join(tokens)
    return ""



def _parse_latex_payload(payload_text: str, *, default_confidence: float) -> tuple[str, float, str]:
    """Parse OCR output from either plain text or JSON.

    External OCR commands/services commonly return either raw LaTeX or a JSON
    object such as {"latex": "...", "confidence": 0.91}.  Supporting both
    formats keeps provider integration simple and testable.
    """

    payload_text = payload_text.strip()
    if not payload_text:
        return "", 0.0, ""
    try:
        payload = json.loads(payload_text)
        if isinstance(payload, dict):
            latex = normalize_latex(str(payload.get("latex") or payload.get("text") or ""))
            confidence = float(payload.get("confidence", default_confidence if latex else 0.0) or 0.0)
            message = str(payload.get("message") or "")
            return latex, confidence, message
    except Exception:
        pass
    return normalize_latex(payload_text), default_confidence, ""


def _ocr_with_command(image: Path, existing: str) -> LatexOcrResult:
    command_template = os.getenv("VERITAS_LATEX_OCR_COMMAND", "").strip()
    if not command_template:
        return LatexOcrResult(existing, "command_not_configured", "command", 0.0, "Set VERITAS_LATEX_OCR_COMMAND with {image} placeholder.")
    if not image.exists():
        return LatexOcrResult(existing, "image_missing", "command", 0.0, f"Image not found: {image}")
    command = command_template.replace("{image}", shlex.quote(str(image)))
    try:
        result = subprocess.run(command, shell=True, text=True, capture_output=True, timeout=int(os.getenv("VERITAS_LATEX_OCR_TIMEOUT_SECS", "60")))
    except Exception as exc:  # noqa: BLE001
        return LatexOcrResult(existing, "command_failed_before_response", "command", 0.0, str(exc))
    if result.returncode != 0:
        return LatexOcrResult(existing, "command_nonzero_exit", "command", 0.0, result.stderr[-500:])
    payload_text = result.stdout.strip()
    latex, confidence, message = _parse_latex_payload(payload_text, default_confidence=0.85)
    return LatexOcrResult(latex or existing, "ocr_complete" if latex else "ocr_empty", "command", confidence if latex else 0.0, message)


def _ocr_with_http(image: Path, existing: str) -> LatexOcrResult:
    url = os.getenv("VERITAS_LATEX_OCR_URL", "").strip()
    if not url:
        return LatexOcrResult(existing, "http_not_configured", "http", 0.0, "Set VERITAS_LATEX_OCR_URL.")
    if not image.exists():
        return LatexOcrResult(existing, "image_missing", "http", 0.0, f"Image not found: {image}")
    try:
        import requests  # type: ignore
        encoded = base64.b64encode(image.read_bytes()).decode("ascii")
        response = requests.post(url, json={"image_base64": encoded, "image_path": str(image)}, timeout=int(os.getenv("VERITAS_LATEX_OCR_TIMEOUT_SECS", "60")))
        payload = response.json() if response.content else {}
    except Exception as exc:  # noqa: BLE001
        return LatexOcrResult(existing, "http_failed_before_response", "http", 0.0, str(exc))
    if response.status_code >= 400:
        return LatexOcrResult(existing, "http_non_success", "http", 0.0, json.dumps(payload)[:500])
    latex = normalize_latex(str(payload.get("latex") or payload.get("text") or ""))
    confidence = float(payload.get("confidence", 0.80 if latex else 0.0) or 0.0)
    return LatexOcrResult(latex or existing, "ocr_complete" if latex else "ocr_empty", "http", confidence)
