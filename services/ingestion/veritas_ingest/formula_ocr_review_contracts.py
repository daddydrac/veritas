from __future__ import annotations

"""Source/mocked Phase 6 contracts for formula OCR and review.

These helpers prove the Formula → OCR → Review → OpenSearch/RDF contract
without requiring live Docker, a real OCR model, or a real PDF renderer.  They
are deliberately deterministic and operate only on temp files/fixtures.
"""

import contextlib
import json
import os
import subprocess
import sys
import types
import tempfile
from pathlib import Path
from typing import Any, Iterator

from rdflib import Graph

from .chunking import make_chunks
from .formula_images import attach_formula_images
from .formulas import extract_formulas
from .human_review import review_citations_in_chunks, review_formulas_noninteractive, review_summary
from .latex_ocr import normalize_latex, ocr_formula_image
from .sinks import chunks_to_turtle, ensure_index

EXPECTED_LATEX = r"E=mc^2"


@contextlib.contextmanager
def patched_env(**updates: str) -> Iterator[None]:
    old = {key: os.environ.get(key) for key in updates}
    try:
        for key, value in updates.items():
            os.environ[key] = value
        yield
    finally:
        for key, value in old.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def write_fake_png(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010804000000b51c0c020000000b4944415478da63fcff1f0003030200efbfa7db0000000049454e44ae426082"))
    return path


def command_ocr_contract(tmp_path: Path) -> dict[str, Any]:
    image = write_fake_png(tmp_path / "formula_E_equals_mc2.png")
    script = tmp_path / "fake_latex_ocr.py"
    script.write_text(
        "import json, sys\n"
        "print(json.dumps({'latex': 'E=mc^2', 'confidence': 0.93, 'message': 'fake command OCR'}))\n",
        encoding="utf-8",
    )
    with patched_env(
        VERITAS_LATEX_OCR_PROVIDER="command",
        VERITAS_LATEX_OCR_COMMAND=f"{sys.executable} {script} {{image}}",
    ):
        result = ocr_formula_image(image, provider="command")
    return {
        "ok": result.status == "ocr_complete" and result.latex == EXPECTED_LATEX and result.confidence >= 0.9,
        "latex": result.latex,
        "status": result.status,
        "engine": result.engine,
        "confidence": result.confidence,
    }



class _FakeHttpResponse:
    status_code = 200
    content = b"{}"

    def json(self) -> dict[str, Any]:
        return {"latex": r"\alpha_i = \beta^2", "confidence": 0.91}


def _fake_requests_module() -> types.SimpleNamespace:
    def post(_url: str, json: dict[str, Any], timeout: int):  # noqa: A002, ARG001
        assert "image_base64" in json
        return _FakeHttpResponse()

    return types.SimpleNamespace(post=post)


@contextlib.contextmanager
def fake_ocr_http_server() -> Iterator[str]:
    """Install a fake requests module so HTTP OCR is tested without threads."""

    original = sys.modules.get("requests")
    sys.modules["requests"] = _fake_requests_module()
    try:
        yield "http://fake-ocr.local/ocr"
    finally:
        if original is None:
            sys.modules.pop("requests", None)
        else:
            sys.modules["requests"] = original

def http_ocr_contract(tmp_path: Path) -> dict[str, Any]:
    image = write_fake_png(tmp_path / "formula_alpha_beta.png")
    with fake_ocr_http_server() as url:
        with patched_env(VERITAS_LATEX_OCR_PROVIDER="http", VERITAS_LATEX_OCR_URL=url):
            result = ocr_formula_image(image, provider="http")
    expected = normalize_latex(r"\alpha_i = \beta^2")
    return {
        "ok": result.status == "ocr_complete" and result.latex == expected and result.confidence >= 0.9,
        "latex": result.latex,
        "status": result.status,
        "engine": result.engine,
        "confidence": result.confidence,
    }


def formula_image_contract(tmp_path: Path) -> dict[str, Any]:
    chunks = [
        {
            "paper_id": "phase6-paper",
            "chunk_id": "phase6-paper::chunk::00000",
            "ordinal": 0,
            "text": "Formula $$E=mc^2$$ appears.",
            "metadata": {"title": "Phase 6 Fixture"},
            "formulas": [
                {
                    "formula_id": "eq-energy",
                    "latex": "E=mc^2",
                    "raw_latex": "$$E=mc^2$$",
                    "source": "docling_visual",
                    "page": 1,
                    "bbox": [10, 10, 100, 40],
                    "confidence": 0.82,
                }
            ],
        }
    ]
    with patched_env(VERITAS_FORMULA_IMAGE_RENDERER="mock", VERITAS_LATEX_OCR_PROVIDER="heuristic"):
        out = attach_formula_images(tmp_path / "missing.pdf", chunks, tmp_path / "formulas")
    formula = out[0]["formulas"][0]
    image_exists = bool(formula.get("formula_image_path")) and Path(formula["formula_image_path"]).exists()
    return {
        "ok": image_exists
        and formula.get("formula_image_status") == "rendered_mock"
        and formula.get("formula_image_engine") == "mock"
        and formula.get("bbox_status") == "bbox_present"
        and formula.get("normalized_latex") == "E=mc^2",
        "formula": formula,
    }


def review_contract(tmp_path: Path) -> dict[str, Any]:
    chunks_path = tmp_path / "chunks.jsonl"
    chunks = [
        {
            "paper_id": "phase6-paper",
            "chunk_id": "phase6-paper::chunk::00000",
            "ordinal": 0,
            "text": "Formula $$E=mc^2$$ appears.",
            "metadata": {
                "paper_id": "phase6-paper",
                "title": "Phase 6 Fixture",
                "apa_citation": "Doe, J. (2026). Phase 6 Fixture.",
                "status": "machine_generated_pending_human_review",
            },
            "formulas": [
                {
                    "formula_id": "eq-energy",
                    "latex": "E=mc^2",
                    "normalized_latex": "E=mc^2",
                    "human_validation_status": "pending_human_review",
                    "human_validated": False,
                    "use_for_codegen": False,
                }
            ],
        }
    ]
    chunks_path.write_text("\n".join(json.dumps(c) for c in chunks), encoding="utf-8")
    formula_result = review_formulas_noninteractive(chunks_path, "approve", reviewer="phase6-test")
    citation_result = review_citations_in_chunks(chunks_path, "edit", corrected_citation="Doe, J. (2026). Corrected Phase 6 Fixture.", reviewer="phase6-test")
    reviewed = [json.loads(line) for line in chunks_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    summary = review_summary(reviewed)
    formula = reviewed[0]["formulas"][0]
    meta = reviewed[0]["metadata"]
    turtle = chunks_to_turtle(reviewed, "https://github.com/daddydrac/veritas/ontology#", "urn:phase6")
    graph = Graph().parse(data=turtle, format="turtle")
    return {
        "ok": formula_result["ok"]
        and citation_result["ok"]
        and formula.get("use_for_codegen") is True
        and formula.get("codegen_eligibility_status") == "eligible_human_validated"
        and meta.get("citation_review_status") == "edit"
        and meta.get("citation_human_validated") is True
        and "Corrected Phase 6" in meta.get("apa_citation", "")
        and len(graph) > 0,
        "summary": summary,
        "formula": formula,
        "metadata": meta,
        "turtle_triples": len(graph),
    }


def chunking_edge_contract() -> dict[str, Any]:
    text = (
        "Dr. Ada wrote a compact derivation with abbreviations e.g. i.e. before the equation "
        "$$x_i = y^2$$; then the system continues without punctuation for many words "
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron "
        "pi rho sigma tau upsilon phi chi psi omega and finally stops. Another formula is "
        "\\[\\sum_i x_i = 1\\]."
    )
    chunks = make_chunks(
        "phase6-chunk",
        text,
        {"title": "Chunk Fixture"},
        target_chars=80,
        overlap_chars=0,
        hard_max_chars=160,
        context_window=20,
    )
    formulas = [formula for chunk in chunks for formula in chunk.get("formulas", [])]
    latex = {formula.get("latex") for formula in formulas}
    boundary_statuses = {chunk.get("boundary_status") for chunk in chunks}
    return {
        "ok": "x_i = y^2" in latex and "\\sum_i x_i = 1" in latex and len(chunks) >= 2,
        "chunk_count": len(chunks),
        "formula_count": len(formulas),
        "boundary_statuses": sorted(str(s) for s in boundary_statuses),
    }


class _FakeIndices:
    def __init__(self):
        self.created_body = None
    def exists(self, index: str) -> bool:  # noqa: ARG002
        return False
    def create(self, index: str, body: dict[str, Any]) -> None:  # noqa: ARG002
        self.created_body = body


class _FakeOpenSearch:
    def __init__(self):
        self.indices = _FakeIndices()


def opensearch_mapping_contract() -> dict[str, Any]:
    client = _FakeOpenSearch()
    ensure_index(client, "phase6", {"services": {"opensearch": {"vector": {"field": "embedding", "dimension": 768}}}})
    props = client.indices.created_body["mappings"]["properties"]
    formulas = props["formulas"]["properties"]
    required_formula_fields = {
        "formula_image_engine",
        "formula_image_confidence",
        "bbox_status",
        "codegen_eligibility_status",
        "review_required",
    }
    required_citation_fields = {"citation_review_status", "citation_human_validated", "citation_reviewer", "citation_usable_for_audit"}
    missing_formula = sorted(required_formula_fields - set(formulas))
    missing_citation = sorted(required_citation_fields - set(props))
    return {"ok": not missing_formula and not missing_citation, "missing_formula": missing_formula, "missing_citation": missing_citation}


def source_mocked_phase6_summary(root: Path | None = None) -> dict[str, Any]:
    root = root or Path.cwd()
    out_dir = root / "data/e2e/source-mocked-formula-ocr-review"
    out_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="veritas-phase6-") as tmp_raw:
        tmp = Path(tmp_raw)
        checks = [
            {"name": "command_ocr_provider_returns_expected_latex", **command_ocr_contract(tmp)},
            {"name": "http_ocr_provider_returns_expected_latex", **http_ocr_contract(tmp)},
            {"name": "mock_formula_image_renderer_creates_metadata", **formula_image_contract(tmp)},
            {"name": "formula_and_citation_review_persist", **review_contract(tmp)},
            {"name": "chunking_edge_cases_preserve_formulas", **chunking_edge_contract()},
            {"name": "opensearch_mapping_contains_phase6_metadata", **opensearch_mapping_contract()},
        ]
    payload = {
        "ok": all(check.get("ok") for check in checks),
        "phase": "phase6_formula_ocr_review",
        "checks": checks,
        "summary": {"checks": len(checks), "passed": sum(1 for check in checks if check.get("ok"))},
    }
    (out_dir / "phase6-summary.json").write_text(json.dumps(payload, indent=2), encoding="utf-8")
    return payload
