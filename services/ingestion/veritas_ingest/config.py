from __future__ import annotations
import os
from pathlib import Path
from typing import Any
import yaml


def _candidate_config_paths(path: str | None = None) -> list[Path]:
    if path:
        return [Path(path)]
    env_path = os.getenv("VERITAS_CONFIG")
    candidates: list[Path] = []
    if env_path:
        candidates.append(Path(env_path))
    candidates.extend([
        Path("/workspace/config/veritas.yaml"),
        Path("config/veritas.yaml"),
        Path.cwd() / "config" / "veritas.yaml",
        Path(__file__).resolve().parents[3] / "config" / "veritas.yaml",
    ])
    seen: set[str] = set()
    unique: list[Path] = []
    for candidate in candidates:
        key = str(candidate)
        if key not in seen:
            unique.append(candidate)
            seen.add(key)
    return unique


def load_config(path: str | None = None) -> dict[str, Any]:
    candidates = _candidate_config_paths(path)
    for p in candidates:
        if p.exists():
            with p.open("r", encoding="utf-8") as f:
                cfg = yaml.safe_load(f)
            cfg.setdefault("_config_path", str(p))
            return cfg
    searched = ", ".join(str(p) for p in candidates)
    raise FileNotFoundError(
        f"Could not find Veritas config. Searched: {searched}. "
        "Pass --config config/veritas.yaml or set VERITAS_CONFIG."
    )


def env_value(name_or_value: str, default: str | None = None) -> str:
    return os.getenv(name_or_value, default or name_or_value)
