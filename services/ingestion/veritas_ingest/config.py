from __future__ import annotations
import os
from pathlib import Path
from typing import Any
import yaml


def load_config(path: str | None = None) -> dict[str, Any]:
    p = Path(path or os.getenv("VERITAS_CONFIG", "/workspace/config/veritas.yaml"))
    with p.open("r", encoding="utf-8") as f:
        cfg = yaml.safe_load(f)
    return cfg


def env_value(name_or_value: str, default: str | None = None) -> str:
    return os.getenv(name_or_value, default or name_or_value)
