from __future__ import annotations

from typing import Any

import requests
from rdflib import DCTERMS, RDF, XSD, Graph, Literal, Namespace


def _vector_config(cfg: dict[str, Any] | None) -> dict[str, Any]:
    if not cfg:
        return {
            "enabled": True,
            "field": "embedding",
            "engine": "faiss",
            "method": "hnsw",
            "space_type": "cosinesimil",
            "dimension": 768,
            "m": 24,
            "ef_construction": 128,
            "ef_search": 100,
        }
    return cfg.get("services", {}).get("opensearch", {}).get("vector", cfg)


def ensure_index(client: Any, index: str, cfg: dict[str, Any] | None = None) -> None:
    """Create the OpenSearch index with FAISS/HNSW vector mapping.

    Acceptance criteria:
        1. Enable OpenSearch k-NN on the index.
        2. Use `knn_vector` with FAISS + HNSW.
        3. Use the configured embedding dimension.
        4. Preserve lexical formula/content search fields.
    """

    if client.indices.exists(index=index):
        return

    vec = _vector_config(cfg)
    field = str(vec.get("field", "embedding"))
    dimension = int(vec.get("dimension", 768))
    engine = str(vec.get("engine", "faiss"))
    method = str(vec.get("method", "hnsw"))
    space_type = str(vec.get("space_type", "cosinesimil"))
    m = int(vec.get("m", 24))
    ef_construction = int(vec.get("ef_construction", 128))
    ef_search = int(vec.get("ef_search", 100))

    body = {
        "settings": {
            "index": {
                "number_of_shards": 1,
                "number_of_replicas": 0,
                "knn": True,
                "knn.algo_param.ef_search": ef_search,
            }
        },
        "mappings": {
            "properties": {
                "chunk_id": {"type": "keyword"},
                "paper_id": {"type": "keyword"},
                "doc_id": {"type": "keyword"},
                "ordinal": {"type": "integer"},
                "chunk_type": {"type": "keyword"},
                "boundary_status": {"type": "keyword"},
                "title": {"type": "text", "fields": {"keyword": {"type": "keyword", "ignore_above": 512}}},
                "abstract": {"type": "text"},
                "apa_citation": {"type": "text", "fields": {"keyword": {"type": "keyword", "ignore_above": 1024}}},
                "citation_review_status": {"type": "keyword"},
                "citation_human_validated": {"type": "boolean"},
                "citation_reviewer": {"type": "keyword"},
                "citation_usable_for_audit": {"type": "boolean"},
                "source_url": {"type": "keyword"},
                "content_hash": {"type": "keyword"},
                "text": {"type": "text"},
                "embedding_model": {"type": "keyword"},
                "embedding_norm": {"type": "float"},
                field: {
                    "type": "knn_vector",
                    "dimension": dimension,
                    "space_type": space_type,
                    "method": {
                        "name": method,
                        "engine": engine,
                        "parameters": {
                            "ef_construction": ef_construction,
                            "m": m,
                        },
                    },
                },
                "formulas": {
                    "type": "nested",
                    "properties": {
                        "formula_id": {"type": "keyword"},
                        "latex": {
                            "type": "text",
                            "fields": {"raw": {"type": "keyword", "ignore_above": 2048}},
                        },
                        "normalized_latex": {
                            "type": "text",
                            "fields": {"raw": {"type": "keyword", "ignore_above": 2048}},
                        },
                        "description": {"type": "text"},
                        "formula_image_path": {"type": "keyword"},
                        "formula_image_status": {"type": "keyword"},
                        "formula_image_engine": {"type": "keyword"},
                        "formula_image_confidence": {"type": "float"},
                        "bbox_status": {"type": "keyword"},
                        "latex_ocr_status": {"type": "keyword"},
                        "latex_ocr_engine": {"type": "keyword"},
                        "latex_ocr_confidence": {"type": "float"},
                        "human_validated": {"type": "boolean"},
                        "human_validation_status": {"type": "keyword"},
                        "use_for_codegen": {"type": "boolean"},
                        "codegen_eligibility_status": {"type": "keyword"},
                        "review_required": {"type": "boolean"},
                        "page": {"type": "integer"},
                        "bbox": {"type": "float"},
                        "raw_latex": {
                            "type": "text",
                            "fields": {"keyword": {"type": "keyword", "ignore_above": 512}},
                        },
                        "start": {"type": "integer"},
                        "end": {"type": "integer"},
                        "source": {"type": "keyword"},
                        "pattern": {"type": "keyword"},
                        "confidence": {"type": "float"},
                    },
                },
                "metadata": {"type": "object", "enabled": True},
            }
        },
    }
    client.indices.create(index=index, body=body)


def index_chunks(opensearch_url: str, index: str, chunks: list[dict], cfg: dict[str, Any] | None = None) -> None:
    """Index chunks into OpenSearch.

    Acceptance criteria:
        1. Create a FAISS/HNSW vector index when absent.
        2. Require embedding vectors before indexing semantic chunks.
        3. Preserve lexical fields and formula metadata.
        4. Refresh the index after all chunks are written.
    """

    from opensearchpy import OpenSearch

    vec = _vector_config(cfg)
    vector_enabled = bool(vec.get("enabled", True))
    field = str(vec.get("field", "embedding"))
    client = OpenSearch(opensearch_url, verify_certs=False, ssl_show_warn=False)
    ensure_index(client, index, vec)
    for chunk in chunks:
        if vector_enabled and field not in chunk:
            raise ValueError(
                f"chunk {chunk.get('chunk_id')} is missing vector field `{field}`; "
                "run embedding before indexing."
            )
        meta = chunk.get("metadata", {}) or {}
        doc = {
            **chunk,
            "doc_id": chunk.get("paper_id"),
            "title": meta.get("title", ""),
            "abstract": meta.get("summary", ""),
            "apa_citation": meta.get("apa_citation", ""),
            "citation_review_status": meta.get("citation_review_status", meta.get("status", "machine_generated_pending_human_review")),
            "citation_human_validated": bool(meta.get("citation_human_validated", False)),
            "citation_reviewer": meta.get("citation_reviewer", ""),
            "citation_usable_for_audit": bool(meta.get("citation_usable_for_audit", False)),
            "source_url": meta.get("source_url") or meta.get("entry_url") or meta.get("pdf_url", ""),
            "content_hash": meta.get("pdf_sha256", ""),
        }
        client.index(index=index, id=chunk["chunk_id"], body=doc, refresh=False)
    client.indices.refresh(index=index)


def _safe_local_name(value: str) -> str:
    s = "".join(ch if ch.isalnum() else "_" for ch in value)
    return s.strip("_") or "item"


def document_graph_uri(cfg: dict[str, Any], paper_id: str, metadata: dict[str, Any]) -> str:
    """Return a deterministic named graph URI for a source document.

    Fuseki stores semantic RDF facts, not PDF binaries. This URI identifies
    the document ABox graph containing SourceDocument, RetrievalResult,
    SymbolicShadow, citation, and formula metadata facts.
    """

    ontology_cfg = cfg.get("ontology", {}) if cfg else {}
    base = str(
        ontology_cfg.get("document_graph_base_uri")
        or ontology_cfg.get("graph_uri")
        or "urn:veritas:graph:document"
    ).rstrip(":/")
    content_hash = str(metadata.get("pdf_sha256") or metadata.get("content_hash") or "").strip()
    suffix = content_hash if content_hash else _safe_local_name(paper_id)
    return f"{base}:{_safe_local_name(suffix)}"


def chunks_to_turtle(chunks: list[dict], namespace: str, graph_uri: str) -> str:
    """Return Turtle graph facts for chunks and formulas."""

    ns = Namespace(namespace if namespace.endswith(("#", "/")) else namespace + "#")
    g = Graph()
    g.bind("veritas", ns)
    g.bind("dcterms", DCTERMS)

    papers: dict[str, dict] = {}
    for c in chunks:
        papers[c["paper_id"]] = c.get("metadata", {})

    for paper_id, meta in papers.items():
        piri = ns[f"paper_{_safe_local_name(paper_id)}"]
        g.add((piri, RDF.type, ns.SourceDocument))
        g.add((piri, DCTERMS.title, Literal(str(meta.get("title", "")))))
        g.add((piri, ns.hasIdentifier, Literal(paper_id)))
        if meta.get("apa_citation"):
            g.add((piri, DCTERMS.bibliographicCitation, Literal(str(meta["apa_citation"]))))
        if meta.get("citation_review_status"):
            g.add((piri, ns.hasCitationReviewStatus, Literal(str(meta.get("citation_review_status")))))
        if meta.get("citation_human_validated") is not None:
            g.add((piri, ns.hasCitationHumanValidationStatus, Literal(str(bool(meta.get("citation_human_validated"))).lower())))
        if meta.get("citation_reviewer"):
            g.add((piri, ns.hasHumanReviewer, Literal(str(meta.get("citation_reviewer")))))
        if meta.get("authors"):
            for author in meta.get("authors", []):
                g.add((piri, DCTERMS.creator, Literal(str(author))))
        if meta.get("year"):
            g.add((piri, DCTERMS.date, Literal(str(meta["year"]))))
        if meta.get("pdf_sha256"):
            g.add((piri, ns.hasContentHash, Literal(str(meta["pdf_sha256"]))))
        if meta.get("pdf_url"):
            g.add((piri, ns.hasSourceUrl, Literal(str(meta["pdf_url"]))))

    for c in chunks:
        ciri = ns[f"chunk_{_safe_local_name(c['chunk_id'])}"]
        piri = ns[f"paper_{_safe_local_name(c['paper_id'])}"]
        g.add((ciri, RDF.type, ns.RetrievalResult))
        g.add((ciri, ns.derivedFrom, piri))
        g.add((ciri, ns.hasIdentifier, Literal(c["chunk_id"])))
        g.add((ciri, ns.hasOrdinal, Literal(int(c.get("ordinal", 0)), datatype=XSD.integer)))
        g.add((ciri, ns.hasText, Literal(c.get("text", ""))))
        if c.get("embedding_model"):
            g.add((ciri, ns.hasEmbeddingModel, Literal(str(c["embedding_model"]))))
        if c.get("embedding_norm") is not None:
            g.add((ciri, ns.hasEmbeddingNorm, Literal(float(c["embedding_norm"]), datatype=XSD.decimal)))
        for i, formula in enumerate(c.get("formulas", [])):
            latex = str(formula.get("latex", ""))
            if not latex.strip():
                continue
            firi = ns[f"formula_{_safe_local_name(c['chunk_id'])}_{i}"]
            g.add((firi, RDF.type, ns.SymbolicShadow))
            g.add((firi, ns.derivedFrom, ciri))
            g.add((firi, ns.hasExpressionText, Literal(latex)))
            if formula.get("formula_id"):
                g.add((firi, ns.hasIdentifier, Literal(str(formula.get("formula_id")))))
            if formula.get("description"):
                g.add((firi, ns.hasDescription, Literal(str(formula.get("description")))))
            if formula.get("raw_latex"):
                g.add((firi, ns.hasRawExpressionText, Literal(str(formula.get("raw_latex")))))
            if formula.get("normalized_latex"):
                g.add((firi, ns.hasNormalizedExpressionText, Literal(str(formula.get("normalized_latex")))))
            if formula.get("formula_image_path"):
                g.add((firi, ns.hasFormulaImagePath, Literal(str(formula.get("formula_image_path")))))
            if formula.get("formula_image_status"):
                g.add((firi, ns.hasFormulaImageStatus, Literal(str(formula.get("formula_image_status")))))
            if formula.get("formula_image_engine"):
                g.add((firi, ns.hasFormulaImageEngine, Literal(str(formula.get("formula_image_engine")))))
            if formula.get("formula_image_confidence") is not None:
                g.add((firi, ns.hasFormulaImageConfidence, Literal(float(formula.get("formula_image_confidence", 0.0)), datatype=XSD.decimal)))
            if formula.get("bbox_status"):
                g.add((firi, ns.hasBoundingBoxStatus, Literal(str(formula.get("bbox_status")))))
            if formula.get("latex_ocr_status"):
                g.add((firi, ns.hasLatexOcrStatus, Literal(str(formula.get("latex_ocr_status")))))
            if formula.get("latex_ocr_engine"):
                g.add((firi, ns.hasLatexOcrEngine, Literal(str(formula.get("latex_ocr_engine")))))
            if formula.get("human_validation_status"):
                g.add((firi, ns.hasHumanValidationStatus, Literal(str(formula.get("human_validation_status")))))
            elif formula.get("human_validated") is not None:
                g.add((firi, ns.hasHumanValidationStatus, Literal(str(bool(formula.get("human_validated"))).lower())))
            if formula.get("codegen_eligibility_status"):
                g.add((firi, ns.hasCodegenEligibilityStatus, Literal(str(formula.get("codegen_eligibility_status")))))
            if formula.get("use_for_codegen") is not None:
                g.add((firi, ns.isEligibleForCodegen, Literal(str(bool(formula.get("use_for_codegen"))).lower())))
            g.add((firi, ns.hasFormulaSource, Literal(str(formula.get("source", "unknown")))))
            if formula.get("pattern"):
                g.add((firi, ns.hasFormulaPattern, Literal(str(formula.get("pattern")))))
            if formula.get("confidence") is not None:
                g.add(
                    (
                        firi,
                        ns.hasConfidenceValue,
                        Literal(float(formula.get("confidence", 0.0)), datatype=XSD.decimal),
                    )
                )
    return g.serialize(format="turtle")


def upload_turtle_to_fuseki(graph_url: str, graph_uri: str, turtle: str, *, append: bool = True) -> None:
    """Upload Turtle to Fuseki.

    Acceptance criteria:
        1. Use POST append mode by default to avoid erasing prior ingests.
        2. Support PUT replacement mode for deterministic test/demo resets.
        3. Raise a clear exception when Fuseki rejects the upload.
    """

    method = requests.post if append else requests.put
    res = method(
        graph_url,
        params={"graph": graph_uri},
        data=turtle.encode("utf-8"),
        headers={"Content-Type": "text/turtle"},
        timeout=120,
    )
    if res.status_code not in (200, 201, 204):
        raise RuntimeError(f"Fuseki upload failed {res.status_code}: {res.text[:1000]}")
