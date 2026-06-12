from __future__ import annotations

"""Local vector and lexical evidence indexes for real local ingestion."""

import json
import math
import re
from pathlib import Path
from typing import Any

from .embeddings import cosine_similarity, l2_norm

_TOKEN_RE = re.compile(r"[A-Za-z0-9_\\^{}=+\-./]+")


def _tokens(text: str) -> list[str]:
    return [tok.lower() for tok in _TOKEN_RE.findall(text or "") if len(tok.strip()) > 1]


def write_local_lexical_index(chunks: list[dict[str, Any]], path: Path) -> dict[str, Any]:
    path.parent.mkdir(parents=True, exist_ok=True)
    rows: list[str] = []
    for chunk in chunks:
        meta = chunk.get("metadata", {}) or {}
        formulas = " ".join(str(formula.get("latex", "")) for formula in chunk.get("formulas", []) or [])
        text = "\n".join([str(meta.get("title", "")), str(chunk.get("text", "")), formulas])
        terms: dict[str, int] = {}
        for token in _tokens(text):
            terms[token] = terms.get(token, 0) + 1
        rows.append(json.dumps({
            "chunk_id": chunk.get("chunk_id"),
            "paper_id": chunk.get("paper_id"),
            "chunk_type": chunk.get("chunk_type", "prose"),
            "title": meta.get("title", ""),
            "term_count": sum(terms.values()),
            "terms": terms,
        }, ensure_ascii=False))
    path.write_text("\n".join(rows), encoding="utf-8")
    return {"status": "available", "path": str(path), "records": len(chunks), "index_type": "lexical_bm25_ready_terms"}


def write_local_vector_index(chunks: list[dict[str, Any]], path: Path) -> dict[str, Any]:
    path.parent.mkdir(parents=True, exist_ok=True)
    vector_chunks = [chunk for chunk in chunks if isinstance(chunk.get("embedding"), list) and chunk.get("embedding")]
    if not vector_chunks:
        path.write_text("", encoding="utf-8")
        return {"status": "unavailable", "path": str(path), "records": 0, "index_type": "cosine_jsonl", "planning_blocked": True}
    dim = len(vector_chunks[0]["embedding"])
    rows: list[str] = []
    for chunk in vector_chunks:
        vec = [float(v) for v in chunk.get("embedding", [])]
        if len(vec) != dim:
            raise ValueError(f"Local vector index dimension mismatch for {chunk.get('chunk_id')}: expected={dim} actual={len(vec)}")
        norm = l2_norm(vec)
        rows.append(json.dumps({
            "chunk_id": chunk.get("chunk_id"),
            "paper_id": chunk.get("paper_id"),
            "chunk_type": chunk.get("chunk_type", "prose"),
            "embedding_model": chunk.get("embedding_model", ""),
            "embedding_norm": norm,
            "embedding": vec,
        }, ensure_ascii=False))
    path.write_text("\n".join(rows), encoding="utf-8")
    return {"status": "available", "path": str(path), "records": len(vector_chunks), "dimension": dim, "index_type": "cosine_jsonl", "planning_blocked": False}


def search_local_vector_index(index_path: Path, query_vector: list[float], *, top_k: int = 10) -> list[dict[str, Any]]:
    if not index_path.exists() or not index_path.read_text(encoding="utf-8").strip():
        return []
    hits: list[dict[str, Any]] = []
    for line in index_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        score = cosine_similarity(query_vector, [float(v) for v in row.get("embedding", [])])
        hits.append({**row, "score": score})
    hits.sort(key=lambda item: item.get("score", -math.inf), reverse=True)
    return hits[:top_k]
