from __future__ import annotations

"""Local embedding provider for the real Veritas local-ingestion path.

This module deliberately does not generate fake vectors.  It either uses a real
local SentenceTransformer model, an explicitly configured HTTP embedding service,
or returns a structured unavailable status so downstream planning can block with
a meaningful remediation message.
"""

from dataclasses import dataclass, asdict
from typing import Any
import os

from .embeddings import attach_embeddings, embedding_text_for_chunk, l2_norm, normalize_vector, DEFAULT_EMBEDDING_MODEL
from .errors import VeritasFailure


@dataclass(frozen=True)
class LocalEmbeddingStatus:
    status: str
    provider: str
    model: str
    dimension: int
    vectors_written: int
    normalize: bool
    planning_blocked: bool
    remediation: str
    error: str = ""

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _cfg_dimension(cfg: dict[str, Any]) -> int:
    emb_cfg = cfg.get("embedding", {}) or cfg.get("services", {}).get("embedding", {}) or {}
    vec_cfg = cfg.get("services", {}).get("opensearch", {}).get("vector", {}) or {}
    return int(emb_cfg.get("dimension") or vec_cfg.get("dimension") or 768)


def _cfg_model(cfg: dict[str, Any]) -> str:
    emb_cfg = cfg.get("embedding", {}) or cfg.get("services", {}).get("embedding", {}) or {}
    return os.getenv(str(emb_cfg.get("model_env", "VERITAS_EMBEDDING_MODEL")), str(emb_cfg.get("default_model", DEFAULT_EMBEDDING_MODEL)))


def _cfg_normalize(cfg: dict[str, Any]) -> bool:
    emb_cfg = cfg.get("embedding", {}) or cfg.get("services", {}).get("embedding", {}) or {}
    return bool(emb_cfg.get("normalize", True))


def _attach_sentence_transformers(chunks: list[dict[str, Any]], cfg: dict[str, Any]) -> tuple[list[dict[str, Any]], LocalEmbeddingStatus]:
    try:
        from sentence_transformers import SentenceTransformer  # type: ignore
    except Exception as exc:  # pragma: no cover - depends on optional local install
        raise VeritasFailure(
            stage="local_embedding.import_sentence_transformers",
            message="sentence-transformers is not installed in this environment.",
            remediation="Install sentence-transformers or set VERITAS_LOCAL_EMBEDDING_PROVIDER=http and start the embedding service.",
            details={"exception": str(exc)},
        ) from exc

    dimension = _cfg_dimension(cfg)
    model_name = _cfg_model(cfg)
    normalize = _cfg_normalize(cfg)
    texts = [embedding_text_for_chunk(chunk, include_formulas=True) for chunk in chunks]
    if any(not text.strip() for text in texts):
        raise VeritasFailure(
            stage="local_embedding.empty_text",
            message="At least one chunk produced empty embedding text.",
            remediation="Inspect PDF parsing and chunking output before embedding.",
        )
    model = SentenceTransformer(model_name)
    vectors = model.encode(texts, normalize_embeddings=normalize, show_progress_bar=False)
    enriched: list[dict[str, Any]] = []
    for chunk, vector in zip(chunks, vectors):
        values = [float(v) for v in list(vector)]
        if len(values) != dimension:
            raise VeritasFailure(
                stage="local_embedding.dimension",
                message=f"Local embedding dimension mismatch: expected={dimension} actual={len(values)}.",
                remediation="Use the configured embedding model/dimension or update the vector index configuration.",
                details={"model": model_name, "chunk_id": chunk.get("chunk_id")},
            )
        if normalize:
            values = normalize_vector(values)
        norm = l2_norm(values)
        updated = dict(chunk)
        updated["embedding"] = values
        updated["embedding_model"] = model_name
        updated["embedding_norm"] = norm
        enriched.append(updated)
    return enriched, LocalEmbeddingStatus(
        status="available",
        provider="sentence-transformers",
        model=model_name,
        dimension=dimension,
        vectors_written=len(enriched),
        normalize=normalize,
        planning_blocked=False,
        remediation="Local embeddings were generated with sentence-transformers.",
    )


def _attach_http(chunks: list[dict[str, Any]], cfg: dict[str, Any]) -> tuple[list[dict[str, Any]], LocalEmbeddingStatus]:
    enriched = attach_embeddings(chunks, cfg)
    return enriched, LocalEmbeddingStatus(
        status="available",
        provider="http",
        model=_cfg_model(cfg),
        dimension=_cfg_dimension(cfg),
        vectors_written=len(enriched),
        normalize=_cfg_normalize(cfg),
        planning_blocked=False,
        remediation="Embeddings were generated through the configured HTTP embedding service.",
    )


def attach_local_embeddings(chunks: list[dict[str, Any]], cfg: dict[str, Any]) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Attach real embeddings for local ingestion when a real provider is available.

    Provider selection:
      - VERITAS_LOCAL_EMBEDDING_PROVIDER=sentence-transformers uses a local HF/SBERT model.
      - VERITAS_LOCAL_EMBEDDING_PROVIDER=http uses the configured embedding service.
      - VERITAS_LOCAL_EMBEDDING_PROVIDER=auto tries sentence-transformers first, then an explicitly configured HTTP service.
      - VERITAS_LOCAL_EMBEDDING_PROVIDER=none records a blocking unavailable status.

    No fake/hash embeddings are produced.  If no real provider works, chunks are
    returned without vectors and the status instructs downstream planning to stop.
    """

    provider = os.getenv("VERITAS_LOCAL_EMBEDDING_PROVIDER", "auto").strip().lower()
    if provider in {"none", "disabled", "off"}:
        return chunks, LocalEmbeddingStatus(
            status="unavailable",
            provider="none",
            model=_cfg_model(cfg),
            dimension=_cfg_dimension(cfg),
            vectors_written=0,
            normalize=_cfg_normalize(cfg),
            planning_blocked=True,
            remediation="Local embeddings are disabled. Install sentence-transformers or enable HTTP embeddings before production-bound planning.",
        ).to_dict()

    errors: list[str] = []
    if provider in {"auto", "sentence-transformers", "sentence_transformers", "local"}:
        try:
            enriched, status = _attach_sentence_transformers(chunks, cfg)
            return enriched, status.to_dict()
        except Exception as exc:  # noqa: BLE001 - collect fallback reason for user-facing status
            errors.append(f"sentence-transformers: {exc}")
            if provider not in {"auto"}:
                return chunks, LocalEmbeddingStatus(
                    status="unavailable",
                    provider="sentence-transformers",
                    model=_cfg_model(cfg),
                    dimension=_cfg_dimension(cfg),
                    vectors_written=0,
                    normalize=_cfg_normalize(cfg),
                    planning_blocked=True,
                    remediation="Install sentence-transformers and model weights, or use VERITAS_LOCAL_EMBEDDING_PROVIDER=http with a running embedding service.",
                    error=str(exc),
                ).to_dict()

    # Avoid silently calling the Docker DNS default in local auto mode.  Use HTTP
    # only when the user explicitly supplied an endpoint or selected http.
    explicit_http = bool(os.getenv("VERITAS_EMBEDDING_URL"))
    if provider in {"auto", "http"} and (provider == "http" or explicit_http):
        try:
            enriched, status = _attach_http(chunks, cfg)
            return enriched, status.to_dict()
        except Exception as exc:  # noqa: BLE001
            errors.append(f"http: {exc}")
            if provider == "http":
                return chunks, LocalEmbeddingStatus(
                    status="unavailable",
                    provider="http",
                    model=_cfg_model(cfg),
                    dimension=_cfg_dimension(cfg),
                    vectors_written=0,
                    normalize=_cfg_normalize(cfg),
                    planning_blocked=True,
                    remediation="Start the embedding service or set VERITAS_LOCAL_EMBEDDING_PROVIDER=sentence-transformers with local model dependencies installed.",
                    error=str(exc),
                ).to_dict()

    return chunks, LocalEmbeddingStatus(
        status="unavailable",
        provider=provider or "auto",
        model=_cfg_model(cfg),
        dimension=_cfg_dimension(cfg),
        vectors_written=0,
        normalize=_cfg_normalize(cfg),
        planning_blocked=True,
        remediation=(
            "No real local embedding provider is available. Install sentence-transformers and model weights, "
            "or start an embedding service and set VERITAS_LOCAL_EMBEDDING_PROVIDER=http plus VERITAS_EMBEDDING_URL. "
            "Ingestion artifacts were written, but production-bound planning must block until embeddings exist."
        ),
        error="; ".join(errors),
    ).to_dict()
