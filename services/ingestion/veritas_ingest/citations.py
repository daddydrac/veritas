from __future__ import annotations

from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any
import re


@dataclass(frozen=True)
class CitationMetadata:
    title: str
    authors: list[str]
    year: str
    source_url: str = ""
    arxiv_id: str = ""
    doi: str = ""
    apa_citation: str = ""
    status: str = "machine_generated_pending_human_review"
    citation_review_status: str = "machine_generated_pending_human_review"
    citation_human_validated: bool = False
    citation_usable_for_audit: bool = False
    citation_reviewer: str = ""


def _year(value: str | None) -> str:
    if not value:
        return "n.d."
    m = re.search(r"(19|20)\d{2}", str(value))
    return m.group(0) if m else "n.d."


def apa_author(author: str) -> str:
    parts = [p for p in re.split(r"\s+", author.strip()) if p]
    if not parts:
        return "Unknown"
    if len(parts) == 1:
        return parts[0]
    family = parts[-1]
    initials = " ".join(f"{p[0]}." for p in parts[:-1] if p)
    return f"{family}, {initials}"


def build_apa_citation(title: str, authors: list[str] | None = None, year: str | None = None, source_url: str = "", doi: str = "") -> str:
    title = (title or "Untitled research artifact").strip().rstrip(".")
    authors = authors or []
    if not authors:
        author_text = "Unknown author"
    elif len(authors) == 1:
        author_text = apa_author(authors[0])
    elif len(authors) <= 20:
        author_text = ", ".join(apa_author(a) for a in authors[:-1]) + ", & " + apa_author(authors[-1])
    else:
        author_text = ", ".join(apa_author(a) for a in authors[:19]) + ", ... " + apa_author(authors[-1])
    y = _year(year)
    locator = doi or source_url
    suffix = f" {locator}" if locator else ""
    return f"{author_text} ({y}). {title}.{suffix}"


def citation_from_metadata(meta: dict[str, Any], pdf_path: Path | None = None) -> dict[str, Any]:
    title = str(meta.get("title") or (pdf_path.stem if pdf_path else "Untitled research artifact"))
    authors_raw = meta.get("authors") or []
    if isinstance(authors_raw, str):
        authors = [a.strip() for a in re.split(r",|;| and ", authors_raw) if a.strip()]
    else:
        authors = [str(a).strip() for a in authors_raw if str(a).strip()]
    year = _year(str(meta.get("published") or meta.get("updated") or meta.get("year") or ""))
    source_url = str(meta.get("entry_url") or meta.get("pdf_url") or meta.get("source_url") or "")
    arxiv_id = str(meta.get("paper_id") or meta.get("arxiv_id") or "")
    doi = str(meta.get("doi") or "")
    apa = build_apa_citation(title, authors, year, source_url=source_url, doi=doi)
    return asdict(CitationMetadata(title=title, authors=authors, year=year, source_url=source_url, arxiv_id=arxiv_id, doi=doi, apa_citation=apa))
