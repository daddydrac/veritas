from __future__ import annotations

from veritas_ingest.embeddings import cosine_similarity, embedding_text_for_chunk, l2_norm, normalize_vector


def test_normalize_vector_returns_unit_norm() -> None:
    vector = [3.0, 4.0]
    normalized = normalize_vector(vector)
    assert vector == [3.0, 4.0]
    assert abs(l2_norm(normalized) - 1.0) < 1e-9


def test_cosine_similarity_for_identical_unit_vectors() -> None:
    assert abs(cosine_similarity([1.0, 0.0], [1.0, 0.0]) - 1.0) < 1e-9


def test_embedding_text_includes_formula_latex() -> None:
    chunk = {
        "text": "We optimize a loss function.",
        "metadata": {"title": "Optimization Paper", "summary": "A test."},
        "formulas": [{"latex": "L(\\theta)=\\sum_i x_i"}],
    }
    text = embedding_text_for_chunk(chunk)
    assert "Optimization Paper" in text
    assert "We optimize" in text
    assert "L(\\theta)" in text
