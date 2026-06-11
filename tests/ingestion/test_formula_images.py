from pathlib import Path

from veritas_ingest.formula_images import attach_formula_images


def test_attach_formula_images_marks_unavailable_without_bbox(tmp_path: Path):
    chunks = [{"paper_id": "p1", "chunk_id": "c1", "formulas": [{"latex": "x=y", "formula_id": "f1"}]}]
    out = attach_formula_images(tmp_path / "missing.pdf", chunks, tmp_path / "formulas")
    formula = out[0]["formulas"][0]
    assert formula["formula_image_status"] == "not_available_no_bbox_or_renderer"
    assert formula["human_validated"] is False
    assert "symbolic shadow" in formula["description"].lower() or "equation" in formula["description"].lower()
