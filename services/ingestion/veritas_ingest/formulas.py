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
