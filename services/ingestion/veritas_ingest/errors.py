from __future__ import annotations

import json
import sys
from dataclasses import asdict, dataclass
from typing import Any


@dataclass(frozen=True)
class VeritasFailure(Exception):
    """Represent a user-actionable Veritas failure.

    Acceptance criteria:
        1. Error messages identify the failing stage.
        2. Error messages include a human-readable explanation.
        3. Error messages include a remediation hint.
        4. Error details are JSON serializable where practical.
    """

    stage: str
    message: str
    remediation: str
    details: dict[str, Any] | None = None

    def __str__(self) -> str:
        return f"{self.stage}: {self.message}"


def emit_failure(error: BaseException, *, stage: str = "unknown") -> None:
    """Print a JSON failure envelope to stderr.

    Acceptance criteria:
        1. Determinism: The same exception fields produce the same JSON shape.
        2. User feedback: The output tells users what failed and how to respond.
        3. Machine readability: The output can be consumed by CLI/API tooling.
    """

    if isinstance(error, VeritasFailure):
        payload = {
            "ok": False,
            "error": asdict(error),
        }
    else:
        payload = {
            "ok": False,
            "error": {
                "stage": stage,
                "message": str(error),
                "remediation": (
                    "Inspect the stack with `docker compose ps` and "
                    "`docker compose logs --tail=200`; rerun with a smaller input "
                    "or verify service configuration."
                ),
                "details": {"exception_type": type(error).__name__},
            },
        }
    print(json.dumps(payload, indent=2, ensure_ascii=False), file=sys.stderr)
