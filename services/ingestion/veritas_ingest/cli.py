from __future__ import annotations

import argparse
import json
import os
from dataclasses import asdict
from pathlib import Path

from .arxiv import ArxivPaper, download_pdf, paper_hash, search_arxiv
from .chunking import make_chunks
from .config import load_config
from .docling_pdf import convert_pdf
from .embeddings import attach_embeddings
from .errors import VeritasFailure, emit_failure
from .formula_images import attach_formula_images
from .human_review import review_formulas_noninteractive
from .codegen import generate_package
from .citations import citation_from_metadata
from .ontology import upload_ontology
from .planning import build_evidence_backed_plan
from .sinks import chunks_to_turtle, document_graph_uri, index_chunks, upload_turtle_to_fuseki


def _service_config(cfg: dict) -> tuple[str, str, str, str]:
    """Return configured storage endpoints.

    Acceptance criteria:
        1. Read service endpoints from environment variables first.
        2. Preserve Docker Compose defaults when environment variables are absent.
        3. Return all values needed to write search and graph outputs.
    """

    opensearch_url = os.getenv("VERITAS_OPENSEARCH_URL", "http://opensearch:9200")
    index = os.getenv("VERITAS_OPENSEARCH_INDEX", "veritas-papers")
    graph_url = os.getenv("VERITAS_FUSEKI_GRAPH_URL", "http://fuseki:3030/veritas/data")
    graph_uri = cfg["ontology"].get(
        "graph_uri",
        "https://github.com/daddydrac/veritas/graph/research",
    )
    return opensearch_url, index, graph_url, graph_uri


def _ingest_pdf(
    *,
    pdf: Path,
    paper_id: str,
    metadata: dict,
    cfg: dict,
) -> list[dict]:
    """Convert, chunk, and return Veritas evidence chunks for a PDF.

    Acceptance criteria:
        1. Validate that the PDF exists and is non-empty.
        2. Preserve parser metadata and formula-extraction metadata.
        3. Produce chunks that can be indexed and converted to RDF.
    """

    if not pdf.exists():
        raise VeritasFailure(
            stage="ingest.validate_pdf",
            message=f"PDF does not exist: {pdf}",
            remediation="Provide a readable local PDF path or use `ingest-arxiv`.",
        )
    if pdf.stat().st_size == 0:
        raise VeritasFailure(
            stage="ingest.validate_pdf",
            message=f"PDF is empty: {pdf}",
            remediation="Download the PDF again and rerun ingestion.",
        )

    ing = cfg["ingestion"]
    pdf_cfg = ing["pdf"]
    chunk_cfg = ing["chunking"]
    out = ing["outputs"]
    docling_dir = Path(out["docling_dir"])

    print(f"[veritas] parsing PDF with Docling-first pipeline: {pdf}")
    converted = convert_pdf(
        pdf,
        docling_dir / paper_id.replace("/", "_"),
        extract_formulas=pdf_cfg.get("extract_formulas", True),
    )
    citation = citation_from_metadata(metadata, pdf)
    metadata = {
        **metadata,
        **citation,
        "pdf_sha256": paper_hash(pdf),
        "pdf_path": str(pdf),
        "parser": converted["parser"],
        "parser_warning": converted.get("fallback_reason", ""),
        "visual_formula_candidates": converted.get("visual_formula_candidates", []),
        "visual_formula_candidates_count": len(converted.get("visual_formula_candidates", [])),
        "docling_formulas_path": converted.get("formulas_path", ""),
    }
    paper_chunks = make_chunks(
        paper_id,
        converted["text"],
        metadata,
        target_chars=int(chunk_cfg["target_chars"]),
        overlap_chars=int(chunk_cfg["overlap_chars"]),
        hard_max_chars=int(chunk_cfg["hard_max_chars"]),
        context_window=int(pdf_cfg["formula_context_window_chars"]),
    )
    formula_image_root = Path(out.get("formula_image_dir", "data/formulas"))
    paper_chunks = attach_formula_images(pdf, paper_chunks, formula_image_root)
    if not paper_chunks:
        raise VeritasFailure(
            stage="ingest.chunk_pdf",
            message=f"No chunks were produced from PDF: {pdf}",
            remediation="Check whether the PDF is scanned-only or unreadable; enable OCR or inspect Docling output.",
            details={"parser": converted["parser"]},
        )
    return paper_chunks


def _write_outputs(chunks: list[dict], cfg: dict) -> None:
    """Write chunks to OpenSearch and Jena/Fuseki.

    Acceptance criteria:
        1. Fail clearly when OpenSearch indexing fails.
        2. Fail clearly when Fuseki graph upload fails.
        3. Persist the latest Turtle graph for auditability.
    """

    opensearch_url, index, graph_url, graph_uri = _service_config(cfg)
    chunks_dir = Path(cfg["ingestion"]["outputs"]["chunks_dir"])
    chunks_dir.mkdir(parents=True, exist_ok=True)

    print(f"[veritas] embedding {len(chunks)} chunks with normalized SBERT vectors")
    try:
        chunks = attach_embeddings(chunks, cfg)
    except Exception as exc:  # noqa: BLE001 - convert to user-facing failure
        raise VeritasFailure(
            stage="ingest.embed_chunks",
            message=f"Embedding generation failed: {exc}",
            remediation="Run `veritas ready`; inspect `docker compose logs embedding`; verify Muennighoff/SBERT-base-nli-v2 can be downloaded and loaded.",
        ) from exc

    print(f"[veritas] indexing {len(chunks)} chunks into OpenSearch FAISS/HNSW index={index}")
    try:
        index_chunks(opensearch_url, index, chunks, cfg)
    except Exception as exc:  # noqa: BLE001 - convert to user-facing failure
        raise VeritasFailure(
            stage="ingest.index_opensearch",
            message=f"OpenSearch indexing failed: {exc}",
            remediation="Run `veritas ready`; inspect `docker compose logs opensearch`; verify OpenSearch 2.19+ and FAISS/HNSW index configuration.",
        ) from exc

    latest_ttls: list[str] = []
    chunks_by_paper: dict[str, list[dict]] = {}
    for chunk in chunks:
        chunks_by_paper.setdefault(str(chunk.get("paper_id", "unknown")), []).append(chunk)

    upload_manifest: list[dict] = []
    for paper_id, paper_chunks in chunks_by_paper.items():
        metadata = paper_chunks[0].get("metadata", {}) if paper_chunks else {}
        target_graph_uri = document_graph_uri(cfg, paper_id, metadata)
        ttl = chunks_to_turtle(paper_chunks, cfg["project"]["namespace"], target_graph_uri)
        safe_name = paper_id.replace("/", "_").replace(":", "_")
        (chunks_dir / f"{safe_name}.ingest.ttl").write_text(ttl, encoding="utf-8")
        latest_ttls.append(ttl)
        print(f"[veritas] uploading RDF document evidence graph to Fuseki graph={target_graph_uri}")
        try:
            upload_turtle_to_fuseki(graph_url, target_graph_uri, ttl, append=True)
        except Exception as exc:  # noqa: BLE001 - convert to user-facing failure
            raise VeritasFailure(
                stage="ingest.upload_fuseki",
                message=f"Fuseki graph upload failed for graph {target_graph_uri}: {exc}",
                remediation="Run `veritas ready`; inspect `docker compose logs fuseki`; verify dataset, graph URL, and Turtle syntax. Re-run ingestion; OpenSearch writes are idempotent by chunk_id.",
                details={"paper_id": paper_id, "graph_uri": target_graph_uri},
            ) from exc
        upload_manifest.append({"paper_id": paper_id, "graph_uri": target_graph_uri, "chunks": len(paper_chunks)})

    (chunks_dir / "latest-ingest.ttl").write_text("\n\n".join(latest_ttls), encoding="utf-8")
    (chunks_dir / "latest-fuseki-upload-manifest.json").write_text(json.dumps(upload_manifest, indent=2), encoding="utf-8")


def cmd_ingest_arxiv(args: argparse.Namespace) -> None:
    cfg = load_config(args.config)
    ing = cfg["ingestion"]
    arx = ing["arxiv"]
    out = ing["outputs"]
    raw_dir = Path(out["raw_pdf_dir"])
    chunks_dir = Path(out["chunks_dir"])
    chunks_dir.mkdir(parents=True, exist_ok=True)

    query = args.query or arx["default_query"]
    max_results = args.max_results or arx["max_results"]
    print(f"[veritas] searching arXiv query={query!r} max_results={max_results}")
    papers = search_arxiv(
        arx["api_url"],
        query,
        max_results,
        arx["sort_by"],
        arx["sort_order"],
    )
    if not papers:
        raise VeritasFailure(
            stage="ingest.search_arxiv",
            message="No arXiv papers matched the query.",
            remediation="Try a broader query such as `cat:cs.AI OR cat:math.OC`.",
            details={"query": query, "max_results": max_results},
        )

    all_chunks: list[dict] = []
    for paper in papers:
        print(f"[veritas] downloading {paper.paper_id}: {paper.title}")
        pdf = download_pdf(paper, raw_dir)
        meta = asdict(paper)
        paper_chunks = _ingest_pdf(pdf=pdf, paper_id=paper.paper_id, metadata=meta, cfg=cfg)
        (chunks_dir / f"{paper.paper_id.replace('/', '_')}.chunks.jsonl").write_text(
            "\n".join(json.dumps(c, ensure_ascii=False) for c in paper_chunks),
            encoding="utf-8",
        )
        all_chunks.extend(paper_chunks)

    _write_outputs(all_chunks, cfg)
    print(
        json.dumps(
            {"ok": True, "papers": len(papers), "chunks": len(all_chunks)},
            indent=2,
        )
    )


def cmd_ingest_pdf(args: argparse.Namespace) -> None:
    cfg = load_config(args.config)
    pdf = Path(args.path)
    paper_id = args.paper_id or pdf.stem
    metadata = {
        "paper_id": paper_id,
        "title": args.title or pdf.stem,
        "summary": args.summary or "Local PDF uploaded into Veritas.",
        "authors": [],
        "published": "",
        "updated": "",
        "pdf_url": "",
        "entry_url": "",
    }
    chunks = _ingest_pdf(pdf=pdf, paper_id=paper_id, metadata=metadata, cfg=cfg)
    chunks_dir = Path(cfg["ingestion"]["outputs"]["chunks_dir"])
    chunks_dir.mkdir(parents=True, exist_ok=True)
    (chunks_dir / f"{paper_id.replace('/', '_')}.chunks.jsonl").write_text(
        "\n".join(json.dumps(c, ensure_ascii=False) for c in chunks),
        encoding="utf-8",
    )
    _write_outputs(chunks, cfg)
    print(json.dumps({"ok": True, "papers": 1, "chunks": len(chunks)}, indent=2))



def cmd_review_formulas(args: argparse.Namespace) -> None:
    """Review formula candidates in a chunks JSONL file.

    Interactive mode is intentionally simple and terminal-safe: the user sees
    LaTeX, description, image path, confidence, and source status, then chooses
    approve/edit/reject/skip. Non-interactive mode is used for CI.
    """

    path = Path(args.chunks)
    if args.decision:
        result = review_formulas_noninteractive(path, args.decision, reviewer=args.reviewer, output=Path(args.output) if args.output else None)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return

    from .human_review import apply_formula_decision, iter_formulas, load_chunks_jsonl, write_chunks_jsonl

    chunks = load_chunks_jsonl(path)
    reviewed = 0
    for _chunk, formula in iter_formulas(chunks):
        print("\nFormula Review")
        print("──────────────")
        print(f"id: {formula.get('formula_id','')}")
        print(f"latex: {formula.get('latex','')}")
        print(f"description: {formula.get('description','')}")
        print(f"image: {formula.get('formula_image_path','') or '<none>'}")
        print(f"source: {formula.get('source','')} confidence={formula.get('confidence','')}")
        decision = input("Decision [approve/edit/reject/skip]: ").strip().lower() or "skip"
        corrected = None
        if decision == "edit":
            corrected = input("Corrected LaTeX: ").strip()
        apply_formula_decision(formula, decision, corrected_latex=corrected, reviewer=args.reviewer)
        reviewed += 1
    target = Path(args.output) if args.output else path
    write_chunks_jsonl(target, chunks)
    print(json.dumps({"ok": True, "formulas_reviewed": reviewed, "path": str(target)}, indent=2, ensure_ascii=False))

def cmd_upload_ontology(args: argparse.Namespace) -> None:
    cfg = load_config(args.config)
    path = Path(args.path or cfg["ontology"].get("file", "/workspace/ontology/veritas.owl"))
    result = upload_ontology(path, cfg)
    print(json.dumps(result, indent=2, ensure_ascii=False))


def cmd_plan(args: argparse.Namespace) -> None:
    cfg = load_config(args.config)
    result = build_evidence_backed_plan(args.prompt, cfg, size=args.size)
    print(json.dumps(result, indent=2, ensure_ascii=False))


def cmd_generate_code(args: argparse.Namespace) -> None:
    cfg = load_config(args.config)
    print("[veritas] WARNING: Python generate-code is legacy scaffold mode. Production codegen uses the Rust API /run path.")
    result = generate_package(args.prompt, args.language, cfg)
    print(json.dumps({
        "ok": True,
        "package_name": result["package_name"],
        "path": result["path"],
        "status": result["status"],
        "next_actions": [
            f"inspect {result['path']}/README.md",
            f"inspect {result['path']}/VALIDATION_REPORT.md",
            "run package-specific tests before production use",
        ],
    }, indent=2, ensure_ascii=False))


def main() -> None:
    parser = argparse.ArgumentParser(prog="veritas-ingest")
    parser.add_argument("--config", default=None)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_arxiv = sub.add_parser("ingest-arxiv")
    p_arxiv.add_argument("--query", required=False)
    p_arxiv.add_argument("--max-results", type=int, required=False)
    p_arxiv.set_defaults(func=cmd_ingest_arxiv)

    p_pdf = sub.add_parser("ingest-pdf")
    p_pdf.add_argument("--path", required=True)
    p_pdf.add_argument("--paper-id", required=False)
    p_pdf.add_argument("--title", required=False)
    p_pdf.add_argument("--summary", required=False)
    p_pdf.set_defaults(func=cmd_ingest_pdf)

    p_onto = sub.add_parser("upload-ontology")
    p_onto.add_argument("--path", required=False)
    p_onto.set_defaults(func=cmd_upload_ontology)

    p_plan = sub.add_parser("plan")
    p_plan.add_argument("--prompt", required=True)
    p_plan.add_argument("--size", type=int, default=8)
    p_plan.set_defaults(func=cmd_plan)

    p_codegen = sub.add_parser("generate-code")
    p_codegen.add_argument("--prompt", required=True)
    p_codegen.add_argument("--language", default="rust")
    p_codegen.set_defaults(func=cmd_generate_code)

    p_review = sub.add_parser("review-formulas")
    p_review.add_argument("--chunks", required=True)
    p_review.add_argument("--decision", choices=["approve", "edit", "reject", "skip", "auto_approve"], required=False)
    p_review.add_argument("--reviewer", default="human")
    p_review.add_argument("--output", required=False)
    p_review.set_defaults(func=cmd_review_formulas)

    args = parser.parse_args()
    try:
        args.func(args)
    except BaseException as exc:  # noqa: BLE001 - CLI must convert to meaningful failure envelope
        emit_failure(exc, stage=f"veritas-ingest.{getattr(args, 'cmd', 'unknown')}")
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
