from __future__ import annotations
from dataclasses import dataclass, asdict
from pathlib import Path
from urllib.parse import urlencode
import hashlib
import requests
from bs4 import BeautifulSoup

@dataclass
class ArxivPaper:
    paper_id: str
    title: str
    summary: str
    authors: list[str]
    published: str
    updated: str
    pdf_url: str
    entry_url: str


def search_arxiv(api_url: str, query: str, max_results: int, sort_by: str, sort_order: str) -> list[ArxivPaper]:
    params = urlencode({"search_query": query, "start": 0, "max_results": max_results, "sortBy": sort_by, "sortOrder": sort_order})
    res = requests.get(f"{api_url}?{params}", timeout=60)
    res.raise_for_status()
    soup = BeautifulSoup(res.text, "xml")
    papers = []
    for entry in soup.find_all("entry"):
        entry_id = entry.id.text.strip()
        paper_id = entry_id.rsplit("/", 1)[-1]
        title = " ".join(entry.title.text.split())
        summary = " ".join(entry.summary.text.split())
        authors = [a.find("name").text for a in entry.find_all("author")]
        pdf_url = ""
        for link in entry.find_all("link"):
            if link.get("title") == "pdf" or link.get("type") == "application/pdf":
                pdf_url = link.get("href")
        papers.append(ArxivPaper(paper_id, title, summary, authors, entry.published.text, entry.updated.text, pdf_url, entry_id))
    return papers


def download_pdf(paper: ArxivPaper, out_dir: Path) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    url = paper.pdf_url or paper.entry_url.replace("/abs/", "/pdf/")
    path = out_dir / f"{paper.paper_id.replace('/', '_')}.pdf"
    if path.exists() and path.stat().st_size > 0:
        return path
    with requests.get(url, timeout=120, stream=True) as res:
        res.raise_for_status()
        with path.open("wb") as f:
            for chunk in res.iter_content(1024 * 1024):
                if chunk:
                    f.write(chunk)
    return path


def paper_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()
