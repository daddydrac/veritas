from __future__ import annotations

import re
from dataclasses import asdict, dataclass

FORMULA_PATTERNS = [
    ("display_dollars", re.compile(r"\$\$(?P<body>.+?)\$\$", re.DOTALL)),
    ("display_brackets", re.compile(r"\\\[(?P<body>.+?)\\\]", re.DOTALL)),
    (
        "latex_environment",
        re.compile(
            r"\\begin\{(?P<env>equation\*?|align\*?|gather\*?|multline\*?)\}"
            r"(?P<body>.+?)\\end\{(?P=env)\}",
            re.DOTALL,
        ),
    ),
    ("inline_dollars", re.compile(r"(?<!\$)\$(?P<body>[^$\n]{2,})\$(?!\$)")),
]

MATH_SIGNAL_RE = re.compile(
    r"(\\[a-zA-Z]+|[_^{}=+\-*/]|\d+\s*[a-zA-Z]|[a-zA-Z]\s*[=<>])"
)


@dataclass(frozen=True)
class Formula:
    """Represent an extracted formula occurrence.

    Attributes:
        latex: Formula body without outer delimiters.
        raw_latex: Formula text including delimiters or environment wrappers.
        start: Start offset in the source text.
        end: End offset in the source text.
        context_before: Text before the formula.
        context_after: Text after the formula.
        source: Extraction source.
        pattern: Matched pattern name.
        confidence: Heuristic extraction confidence in [0, 1].
    """

    latex: str
    raw_latex: str
    start: int
    end: int
    context_before: str
    context_after: str
    source: str = "regex_or_docling"
    pattern: str = "unknown"
    confidence: float = 0.55


def _is_probably_math(body: str, pattern_name: str) -> bool:
    """Return whether a formula body has enough mathematical signal.

    Acceptance criteria:
        1. Display formulas are accepted unless empty.
        2. Inline dollar snippets need math-like syntax to avoid currency false positives.
        3. Function is deterministic and does not mutate inputs.
    """

    stripped = body.strip()
    if len(stripped) < 2:
        return False
    if pattern_name != "inline_dollars":
        return True
    if stripped.replace(",", "").replace(".", "").isdigit():
        return False
    return bool(MATH_SIGNAL_RE.search(stripped))


def _confidence(pattern_name: str, body: str) -> float:
    if pattern_name in {"display_dollars", "display_brackets", "latex_environment"}:
        return 0.86
    return 0.68 if MATH_SIGNAL_RE.search(body) else 0.45


def extract_formulas(text: str, context_window: int = 650) -> list[dict]:
    """Extract formula-like LaTeX from text.

    Acceptance criteria:
        1. Preserve start/end offsets against the original text.
        2. Preserve both formula body and raw delimited expression.
        3. Include surrounding context for later semantic analysis.
        4. Avoid obvious inline-dollar non-math false positives.
    """

    seen: list[tuple[int, int]] = []
    formulas: list[Formula] = []
    for pattern_name, pattern in FORMULA_PATTERNS:
        for match in pattern.finditer(text):
            start, end = match.span()
            if any(max(start, s) < min(end, e) for s, e in seen):
                continue
            body = match.group("body").strip()
            if not _is_probably_math(body, pattern_name):
                continue
            seen.append((start, end))
            formulas.append(
                Formula(
                    latex=body,
                    raw_latex=match.group(0),
                    start=start,
                    end=end,
                    context_before=text[max(0, start - context_window) : start].strip(),
                    context_after=text[end : min(len(text), end + context_window)].strip(),
                    pattern=pattern_name,
                    confidence=_confidence(pattern_name, body),
                )
            )
    formulas.sort(key=lambda f: f.start)
    return [asdict(f) for f in formulas]


def extract_docling_formula_candidates(docling_payload: dict, context_window: int = 650) -> list[dict]:
    """Extract visual/formula candidates from a Docling export dictionary.

    Docling's JSON structure evolves, so this walker is intentionally tolerant:
    it looks for objects whose labels/types indicate formula/equation/math and
    preserves any text, latex-like content, page, and bbox/provenance metadata it
    can find. It never raises for schema drift; it returns auditable candidates.
    """

    candidates: list[dict] = []

    def walk(obj, path: str = "") -> None:
        if isinstance(obj, dict):
            lower_values = " ".join(str(obj.get(k, "")).lower() for k in ("label", "type", "kind", "name", "category"))
            text_value = str(obj.get("text") or obj.get("latex") or obj.get("formula") or obj.get("content") or "").strip()
            looks_formula = any(token in lower_values for token in ("formula", "equation", "math")) or (text_value and _is_probably_math(text_value, "docling_visual"))
            if looks_formula and text_value:
                bbox = _extract_bbox(obj)
                page = _extract_page(obj)
                candidates.append({
                    "latex": text_value,
                    "raw_latex": text_value,
                    "start": None,
                    "end": None,
                    "context_before": "",
                    "context_after": "",
                    "source": "docling_visual",
                    "pattern": "docling_visual_formula",
                    "confidence": 0.82 if bbox else 0.72,
                    "page": page,
                    "bbox": bbox,
                    "docling_path": path,
                    "visual_candidate": True,
                })
            for key, value in obj.items():
                walk(value, f"{path}.{key}" if path else str(key))
        elif isinstance(obj, list):
            for index, item in enumerate(obj):
                walk(item, f"{path}[{index}]")

    walk(docling_payload)
    return _dedupe_formula_candidates(candidates)


def merge_formula_candidates(regex_formulas: list[dict], visual_candidates: list[dict]) -> list[dict]:
    """Merge regex and visual candidates without duplicating identical LaTeX."""

    merged = [dict(f) for f in regex_formulas]
    seen = {str(f.get("latex", "")).strip() for f in merged if str(f.get("latex", "")).strip()}
    for candidate in visual_candidates:
        latex = str(candidate.get("latex", "")).strip()
        if not latex or latex in seen:
            continue
        merged.append(dict(candidate))
        seen.add(latex)
    return merged


def _dedupe_formula_candidates(candidates: list[dict]) -> list[dict]:
    seen: set[tuple[str, str]] = set()
    out: list[dict] = []
    for item in candidates:
        key = (str(item.get("latex", "")).strip(), str(item.get("bbox", "")))
        if key in seen:
            continue
        seen.add(key)
        out.append(item)
    return out


def _extract_page(obj: dict) -> int | None:
    for key in ("page", "page_no", "page_number"):
        value = obj.get(key)
        if isinstance(value, int):
            return value
        if isinstance(value, str) and value.isdigit():
            return int(value)
    prov = obj.get("prov") or obj.get("provenance")
    if isinstance(prov, list) and prov:
        return _extract_page(prov[0])
    if isinstance(prov, dict):
        return _extract_page(prov)
    return None


def _extract_bbox(obj: dict) -> list[float] | None:
    for key in ("bbox", "bounding_box", "box"):
        value = obj.get(key)
        parsed = _parse_bbox(value)
        if parsed:
            return parsed
    prov = obj.get("prov") or obj.get("provenance")
    if isinstance(prov, list) and prov:
        return _extract_bbox(prov[0])
    if isinstance(prov, dict):
        return _extract_bbox(prov)
    return None


def _parse_bbox(value) -> list[float] | None:
    if isinstance(value, dict):
        keys = ("l", "t", "r", "b") if all(k in value for k in ("l", "t", "r", "b")) else ("x0", "y0", "x1", "y1")
        if all(k in value for k in keys):
            try:
                return [float(value[k]) for k in keys]
            except Exception:
                return None
    if isinstance(value, list) and len(value) >= 4:
        try:
            return [float(value[0]), float(value[1]), float(value[2]), float(value[3])]
        except Exception:
            return None
    return None
