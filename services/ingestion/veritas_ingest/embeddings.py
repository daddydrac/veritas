from __future__ import annotations

import math
import os
from collections.abc import Iterable
from typing import Any

import requests

from .errors import VeritasFailure


DEFAULT_EMBEDDING_MODEL = "Muennighoff/SBERT-base-nli-v2"


def l2_norm(vector: Iterable[float]) -> float:
    """Return the L2 norm for a vector.

    Acceptance criteria:
        1. Determinism: The same vector returns the same norm.
        2. No mutation: Do not mutate caller-owned vectors.
        3. Empty handling: Empty vectors return zero.
    """

    return math.sqrt(sum(float(value) * float(value) for value in vector))


def normalize_vector(vector: list[float]) -> list[float]:
    """Return a unit-length copy of a vector.

    Acceptance criteria:
        1. Determinism: The same vector returns the same normalized values.
        2. No mutation: Do not mutate caller-owned vectors.
        3. Validation: A zero vector raises `VeritasFailure`.
        4. Cosine readiness: Returned vectors have norm approximately one.
    """

    norm = l2_norm(vector)
    if norm == 0.0:
        raise VeritasFailure(
            stage="embedding.normalize",
            message="Cannot normalize a zero-length embedding vector.",
            remediation="Check the embedding model output and remove empty chunk texts before indexing.",
        )
    return [float(value) / norm for value in vector]


def cosine_similarity(left: list[float], right: list[float]) -> float:
    """Return cosine similarity for two vectors.

    Acceptance criteria:
        1. Determinism: The same inputs produce the same score.
        2. No mutation: Do not mutate caller-owned vectors.
        3. Validation: Mismatched or zero vectors raise `VeritasFailure`.
        4. Correctness: Unit vectors are scored by dot product.
    """

    if len(left) != len(right):
        raise VeritasFailure(
            stage="embedding.cosine",
            message=f"Embedding dimension mismatch: left={len(left)} right={len(right)}.",
            remediation="Use one embedding model and dimension for both query and indexed chunks.",
        )
    left_norm = l2_norm(left)
    right_norm = l2_norm(right)
    if left_norm == 0.0 or right_norm == 0.0:
        raise VeritasFailure(
            stage="embedding.cosine",
            message="Cannot evaluate cosine similarity with a zero vector.",
            remediation="Check empty chunk/query text and embedding service output.",
        )
    return sum(float(a) * float(b) for a, b in zip(left, right)) / (left_norm * right_norm)


def embedding_text_for_chunk(chunk: dict[str, Any], *, include_formulas: bool = True) -> str:
    """Return the text that should be embedded for a chunk.

    Acceptance criteria:
        1. Include the human-readable title when present.
        2. Include chunk text.
        3. Include formula LaTeX when `include_formulas` is true.
        4. Never return an empty string for a non-empty chunk.
    """

    meta = chunk.get("metadata", {}) or {}
    parts: list[str] = []
    title = str(meta.get("title", "")).strip()
    if title:
        parts.append(f"Title: {title}")
    summary = str(meta.get("summary", "")).strip()
    if summary:
        parts.append(f"Summary: {summary}")
    text = str(chunk.get("text", "")).strip()
    if text:
        parts.append(text)
    if include_formulas:
        formulas = [
            str(formula.get("latex", "")).strip()
            for formula in chunk.get("formulas", [])
            if str(formula.get("latex", "")).strip()
        ]
        if formulas:
            parts.append("Formulas:\n" + "\n".join(formulas))
    return "\n\n".join(parts).strip()


def _embedding_url(cfg: dict[str, Any]) -> str:
    embedding_cfg = cfg.get("services", {}).get("embedding", {}) or cfg.get("services", {}).get("model", {})
    url_env = cfg.get("services", {}).get("embedding", {}).get("url_env", "VERITAS_EMBEDDING_URL")
    return os.getenv(url_env, "http://embedding:8090")


def _embedding_cfg(cfg: dict[str, Any]) -> dict[str, Any]:
    return cfg.get("services", {}).get("embedding", {}) or cfg.get("model", {}).get("embedding", {}) or cfg.get("embedding", {}) or {}


def _post_embeddings(url: str, texts: list[str], *, normalize: bool, batch_size: int) -> dict[str, Any]:
    try:
        response = requests.post(
            url.rstrip("/") + "/embed",
            json={"texts": texts, "normalize": normalize, "batch_size": batch_size},
            timeout=600,
        )
    except requests.RequestException as exc:
        raise VeritasFailure(
            stage="embedding.request",
            message=f"Embedding service request failed: {exc}",
            remediation="Run `veritas ready`; inspect `docker compose logs embedding`; verify the embedding model can be downloaded.",
        ) from exc
    if response.status_code >= 400:
        raise VeritasFailure(
            stage="embedding.response",
            message=f"Embedding service returned HTTP {response.status_code}: {response.text[:1000]}",
            remediation="Inspect `docker compose logs embedding` and verify input chunk text is not empty.",
        )
    return response.json()


def attach_embeddings(chunks: list[dict[str, Any]], cfg: dict[str, Any]) -> list[dict[str, Any]]:
    """Return chunks with normalized SBERT embeddings attached.

    Acceptance criteria:
        1. Use the configured Hugging Face/SBERT embedding service.
        2. Include formula LaTeX in the embedded text by default.
        3. Validate embedding dimension against config.
        4. Validate unit-length normalization for cosine similarity.
        5. Return new chunk dictionaries instead of mutating caller-owned chunks.
    """

    if not chunks:
        return []

    emb_cfg = cfg.get("embedding", {})
    vec_cfg = cfg.get("services", {}).get("opensearch", {}).get("vector", {})
    dimension = int(emb_cfg.get("dimension") or vec_cfg.get("dimension") or 768)
    batch_size = int(emb_cfg.get("batch_size", 16))
    normalize = bool(emb_cfg.get("normalize", True))
    norm_tolerance = float(emb_cfg.get("norm_tolerance") or vec_cfg.get("norm_tolerance") or 0.001)
    include_formulas = bool(emb_cfg.get("include_formula_text", True))
    embedding_url = os.getenv(str(emb_cfg.get("url_env", "VERITAS_EMBEDDING_URL")), "http://embedding:8090")
    model_name = os.getenv(str(emb_cfg.get("model_env", "VERITAS_EMBEDDING_MODEL")), str(emb_cfg.get("default_model", DEFAULT_EMBEDDING_MODEL)))

    enriched: list[dict[str, Any]] = []
    for start in range(0, len(chunks), batch_size):
        batch = chunks[start : start + batch_size]
        texts = [embedding_text_for_chunk(chunk, include_formulas=include_formulas) for chunk in batch]
        if any(not text for text in texts):
            raise VeritasFailure(
                stage="embedding.prepare_text",
                message="At least one chunk produced empty embedding text.",
                remediation="Inspect chunking output and parser extraction for empty chunks.",
                details={"batch_start": start},
            )
        payload = _post_embeddings(embedding_url, texts, normalize=normalize, batch_size=batch_size)
        vectors = payload.get("vectors", [])
        norms = payload.get("norms", [])
        if len(vectors) != len(batch):
            raise VeritasFailure(
                stage="embedding.validate_count",
                message=f"Embedding service returned {len(vectors)} vectors for {len(batch)} texts.",
                remediation="Retry ingestion; inspect embedding service logs for partial batch failure.",
            )
        for chunk, text, vector, norm in zip(batch, texts, vectors, norms):
            if len(vector) != dimension:
                raise VeritasFailure(
                    stage="embedding.validate_dimension",
                    message=f"Embedding dimension mismatch for {chunk.get('chunk_id')}: expected={dimension} actual={len(vector)}.",
                    remediation="Use the configured embedding model or update OpenSearch vector dimension before indexing.",
                    details={"model": model_name, "chunk_id": chunk.get("chunk_id")},
                )
            vector = [float(value) for value in vector]
            norm = float(norm)
            if normalize and abs(norm - 1.0) > norm_tolerance:
                vector = normalize_vector(vector)
                norm = l2_norm(vector)
            if normalize and abs(norm - 1.0) > norm_tolerance:
                raise VeritasFailure(
                    stage="embedding.validate_norm",
                    message=f"Embedding norm is not close to 1.0 for cosine search: norm={norm:.6f}.",
                    remediation="Ensure the embedding service is called with normalize=true and the model output is not corrupted.",
                    details={"chunk_id": chunk.get("chunk_id"), "norm_tolerance": norm_tolerance},
                )
            updated = {**chunk}
            updated["embedding"] = vector
            updated["embedding_model"] = model_name
            updated["embedding_norm"] = norm
            updated["embedding_text_sha256_source"] = text[:1000]
            enriched.append(updated)
    return enriched
