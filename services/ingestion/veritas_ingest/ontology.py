from __future__ import annotations

import os
from pathlib import Path
from typing import Any

import requests

from .errors import VeritasFailure


def upload_ontology(path: Path, cfg: dict[str, Any]) -> dict[str, Any]:
    """Upload an OWL/RDF ontology into Fuseki as a named graph.

    Acceptance criteria:
        1. Validate the ontology file exists and is non-empty.
        2. Use a separate ontology graph URI from research evidence graph.
        3. Return a structured success payload for CLI/API automation.
    """

    if not path.exists():
        raise VeritasFailure(
            stage="ontology.validate_file",
            message=f"Ontology file does not exist: {path}",
            remediation="Use `veritas upload-ontology` from the repo root or pass --path to an existing OWL/RDF file.",
        )
    if path.stat().st_size == 0:
        raise VeritasFailure(
            stage="ontology.validate_file",
            message=f"Ontology file is empty: {path}",
            remediation="Provide a non-empty OWL/RDF ontology file.",
        )
    graph_url = os.getenv("VERITAS_FUSEKI_GRAPH_URL", "http://fuseki:3030/veritas/data")
    graph_uri = cfg.get("ontology", {}).get(
        "ontology_graph_uri",
        "https://github.com/daddydrac/veritas/graph/ontology",
    )
    data = path.read_bytes()
    content_type = "application/rdf+xml" if path.suffix.lower() in {".owl", ".rdf", ".xml"} else "text/turtle"
    try:
        response = requests.put(
            graph_url,
            params={"graph": graph_uri},
            data=data,
            headers={"Content-Type": content_type},
            timeout=120,
        )
    except requests.RequestException as exc:
        raise VeritasFailure(
            stage="ontology.upload_transport",
            message=f"Ontology upload request failed: {exc}",
            remediation="Run `veritas ready`; inspect `docker compose logs fuseki`; verify Fuseki graph-store endpoint.",
        ) from exc
    if response.status_code not in {200, 201, 204}:
        raise VeritasFailure(
            stage="ontology.upload_response",
            message=f"Fuseki rejected ontology upload HTTP {response.status_code}: {response.text[:1000]}",
            remediation="Validate the ontology syntax and content type, then retry.",
            details={"graph_uri": graph_uri, "content_type": content_type},
        )
    return {"ok": True, "graph_uri": graph_uri, "path": str(path), "bytes": len(data)}
