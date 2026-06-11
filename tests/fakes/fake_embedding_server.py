from __future__ import annotations

import math
import os
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI(title="Veritas fake embedding service")


class EmbedRequest(BaseModel):
    texts: list[str]
    normalize: bool | None = True
    batch_size: int | None = None


@app.get("/health")
def health():
    return {"ok": True, "service": "fake-embedding", "dimension": int(os.getenv("VERITAS_FAKE_EMBEDDING_DIM", "768"))}


def _vector(text: str, dim: int) -> list[float]:
    seed = sum(ord(c) for c in text) or 1
    raw = [((seed + 31 * i) % 997) / 997.0 - 0.5 for i in range(dim)]
    norm = math.sqrt(sum(x * x for x in raw)) or 1.0
    return [x / norm for x in raw]


@app.post("/embed")
def embed(req: EmbedRequest):
    dim = int(os.getenv("VERITAS_FAKE_EMBEDDING_DIM", "768"))
    return {"vectors": [_vector(text, dim) for text in req.texts], "model": "fake-sbert", "normalized": True}
