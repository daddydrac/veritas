from __future__ import annotations
from pathlib import Path
from typing import Any
import json


def convert_pdf(pdf_path: Path, out_dir: Path, extract_formulas: bool = True) -> dict[str, Any]:
    """Convert PDF to structured markdown/json.

    Uses Docling first. Falls back to pypdf text extraction when Docling is unavailable
    or fails. Formula recognition in Docling is improving rapidly; Veritas always runs
    a second regex/context formula pass over the exported Markdown to preserve math.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = pdf_path.stem
    md_path = out_dir / f"{stem}.md"
    json_path = out_dir / f"{stem}.json"
    try:
        from docling.datamodel.pipeline_options import PdfPipelineOptions
        from docling.document_converter import DocumentConverter, PdfFormatOption
        from docling.datamodel.base_models import InputFormat

        pipeline_options = PdfPipelineOptions()
        pipeline_options.do_ocr = True
        # Best-effort flags; Docling APIs evolve, so keep guarded.
        if hasattr(pipeline_options, "do_formula_enrichment"):
            pipeline_options.do_formula_enrichment = extract_formulas
        if hasattr(pipeline_options, "do_table_structure"):
            pipeline_options.do_table_structure = True
        converter = DocumentConverter(format_options={InputFormat.PDF: PdfFormatOption(pipeline_options=pipeline_options)})
        result = converter.convert(str(pdf_path))
        doc = result.document
        markdown = doc.export_to_markdown()
        md_path.write_text(markdown, encoding="utf-8")
        try:
            json_path.write_text(json.dumps(doc.export_to_dict(), ensure_ascii=False, indent=2), encoding="utf-8")
        except Exception:
            json_path.write_text(json.dumps({"warning": "Docling export_to_dict unavailable"}, indent=2), encoding="utf-8")
        return {"parser": "docling", "text": markdown, "markdown_path": str(md_path), "json_path": str(json_path)}
    except Exception as exc:
        from pypdf import PdfReader
        reader = PdfReader(str(pdf_path))
        text = "\n\n".join((page.extract_text() or "") for page in reader.pages)
        md_path.write_text(text, encoding="utf-8")
        json_path.write_text(json.dumps({"parser": "pypdf", "fallback_reason": str(exc)}, indent=2), encoding="utf-8")
        return {"parser": "pypdf", "text": text, "markdown_path": str(md_path), "json_path": str(json_path), "fallback_reason": str(exc)}
