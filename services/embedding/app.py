from __future__ import annotations

import math
import os
from functools import lru_cache
from typing import Literal

import numpy as np
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from sentence_transformers import SentenceTransformer


DEFAULT_MODEL = "Muennighoff/SBERT-base-nli-v2"


class EmbedRequest(BaseModel):
    """Request payload for embedding generation."""

    texts: list[str] = Field(min_length=1)
    normalize: bool | None = None
    batch_size: int | None = Field(default=None, ge=1, le=1024)


class PairwiseCosineRequest(BaseModel):
    """Request payload for pairwise cosine evaluation."""

    left: str
    right: str
    normalize: bool | None = None


class EmbedResponse(BaseModel):
    """Embedding response payload."""

    model: str
    dimension: int
    normalized: bool
    vectors: list[list[float]]
    norms: list[float]


class CosineResponse(BaseModel):
    """Cosine evaluation response payload."""

    model: str
    normalized: bool
    cosine: float
    left_norm: float
    right_norm: float


app = FastAPI(title="Veritas Embedding Service")


def _env_bool(name: str, default: bool) -> bool:
    value = os.getenv(name)
    if value is None:
        return default
    return value.strip().lower() in {"1", "true", "yes", "y", "on"}


def _device() -> str | None:
    device = os.getenv("VERITAS_EMBEDDING_DEVICE", "auto").strip().lower()
    if not device or device == "auto":
        return None
    return device


@lru_cache(maxsize=1)
def _model() -> SentenceTransformer:
    model_name = os.getenv("VERITAS_EMBEDDING_MODEL", DEFAULT_MODEL)
    device = _device()
    if device is None:
        return SentenceTransformer(model_name)
    return SentenceTransformer(model_name, device=device)


def _norm(vector: list[float]) -> float:
    return math.sqrt(sum(value * value for value in vector))


def _as_float_vectors(embeddings: np.ndarray) -> list[list[float]]:
    return [[float(value) for value in row] for row in embeddings]


def _encode(texts: list[str], normalize: bool | None, batch_size: int | None) -> tuple[bool, list[list[float]], list[float]]:
    resolved_normalize = _env_bool("VERITAS_EMBEDDING_NORMALIZE", True) if normalize is None else normalize
    resolved_batch_size = batch_size or int(os.getenv("VERITAS_EMBEDDING_BATCH_SIZE", "16"))
    embeddings = _model().encode(
        texts,
        batch_size=resolved_batch_size,
        convert_to_numpy=True,
        normalize_embeddings=resolved_normalize,
        show_progress_bar=False,
    )
    vectors = _as_float_vectors(embeddings.astype(np.float32, copy=False))
    norms = [_norm(vector) for vector in vectors]
    return resolved_normalize, vectors, norms


@app.get("/health")
def health() -> dict[str, str | int | bool]:
    model = _model()
    dimension = int(model.get_sentence_embedding_dimension() or 0)
    return {
        "ok": True,
        "service": "veritas-embedding",
        "model": os.getenv("VERITAS_EMBEDDING_MODEL", DEFAULT_MODEL),
        "dimension": dimension,
        "normalized_default": _env_bool("VERITAS_EMBEDDING_NORMALIZE", True),
    }


@app.post("/embed", response_model=EmbedResponse)
def embed(request: EmbedRequest) -> EmbedResponse:
    if any(not text.strip() for text in request.texts):
        raise HTTPException(
            status_code=400,
            detail={
                "code": "embedding.empty_text",
                "message": "Embedding input contains an empty text item.",
                "remediation": "Remove empty strings before calling /embed.",
            },
        )
    normalized, vectors, norms = _encode(request.texts, request.normalize, request.batch_size)
    dimension = len(vectors[0]) if vectors else 0
    return EmbedResponse(
        model=os.getenv("VERITAS_EMBEDDING_MODEL", DEFAULT_MODEL),
        dimension=dimension,
        normalized=normalized,
        vectors=vectors,
        norms=norms,
    )


@app.post("/cosine", response_model=CosineResponse)
def cosine(request: PairwiseCosineRequest) -> CosineResponse:
    normalized, vectors, norms = _encode([request.left, request.right], request.normalize, batch_size=2)
    if len(vectors) != 2:
        raise HTTPException(status_code=500, detail="Embedding service returned an unexpected vector count.")
    left = np.asarray(vectors[0], dtype=np.float32)
    right = np.asarray(vectors[1], dtype=np.float32)
    denom = float(np.linalg.norm(left) * np.linalg.norm(right))
    score = float(np.dot(left, right) / denom) if denom else 0.0
    return CosineResponse(
        model=os.getenv("VERITAS_EMBEDDING_MODEL", DEFAULT_MODEL),
        normalized=normalized,
        cosine=score,
        left_norm=norms[0],
        right_norm=norms[1],
    )
