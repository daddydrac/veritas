from __future__ import annotations

"""Local RDF persistence for real local ingestion."""

from pathlib import Path
from typing import Any

from .sinks import chunks_to_turtle, document_graph_uri


def write_local_rdf(chunks: list[dict[str, Any]], cfg: dict[str, Any], workspace: Path, *, paper_id: str) -> dict[str, Any]:
    workspace.mkdir(parents=True, exist_ok=True)
    metadata = chunks[0].get("metadata", {}) if chunks else {}
    graph_uri = document_graph_uri(cfg, paper_id, metadata)
    ttl = chunks_to_turtle(chunks, cfg["project"]["namespace"], graph_uri)
    evidence_path = workspace / "evidence.ttl"
    evidence_path.write_text(ttl, encoding="utf-8")
    latest_path = workspace / "latest-ingest.ttl"
    latest_path.write_text(ttl, encoding="utf-8")
    return {"status": "available", "path": str(evidence_path), "graph_uri": graph_uri, "bytes": len(ttl.encode("utf-8"))}
