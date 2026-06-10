#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import yaml
from rdflib import Graph

ROOT = Path(__file__).resolve().parents[1]
INGESTION_SRC = ROOT / "services" / "ingestion"
sys.path.insert(0, str(INGESTION_SRC))

from veritas_ingest.chunking import make_chunks  # noqa: E402
from veritas_ingest.formulas import extract_formulas  # noqa: E402
from veritas_ingest.sinks import chunks_to_turtle  # noqa: E402


REQUIRED_FILES = [
    "docker-compose.yml",
    ".env.example",
    "apps/api/src/main.rs",
    "apps/cli/src/main.rs",
    "services/ingestion/veritas_ingest/cli.py",
    "services/ingestion/veritas_ingest/docling_pdf.py",
    "services/ingestion/veritas_ingest/formulas.py",
    "services/ingestion/veritas_ingest/chunking.py",
    "services/ingestion/veritas_ingest/sinks.py",
    "packages/ontology/veritas.owl",
    "packages/ontology/queries/evidence_chunks.sparql",
    "packages/ontology/queries/formula_traceability.sparql",
    "docs/architecture/VERITAS_SPEC.md",
    "docs/architecture/END_TO_END_WORKFLOW.md",
    "README.md",
    "QUICKSTART.md",
    "docs/MODELS.md",
    "MODEL_SERVING_UPDATE.md",
    "services/ingestion/veritas_ingest/planning.py",
    "services/ingestion/veritas_ingest/codegen.py",
    "services/ingestion/veritas_ingest/ontology.py",
]


def check_file_exists(rel: str) -> dict:
    path = ROOT / rel
    return {"name": f"file:{rel}", "ok": path.exists(), "details": str(path)}


def check_yaml() -> dict:
    try:
        yaml.safe_load((ROOT / "config/veritas.yaml").read_text(encoding="utf-8"))
        yaml.safe_load((ROOT / "docker-compose.yml").read_text(encoding="utf-8"))
        return {"name": "yaml.parse", "ok": True}
    except Exception as exc:  # noqa: BLE001
        return {"name": "yaml.parse", "ok": False, "details": str(exc)}


def check_formula_extraction() -> dict:
    text = "Let $$E=mc^2$$ and $x_i = y^2$ hold. The price is $20."
    formulas = extract_formulas(text)
    bodies = [f["latex"] for f in formulas]
    ok = "E=mc^2" in bodies and "x_i = y^2" in bodies and "20" not in bodies
    return {"name": "formula.extraction", "ok": ok, "details": bodies}


def check_chunk_formula_boundary() -> dict:
    text = "Intro. " + "a" * 40 + " $$" + "x" * 200 + "=1$$ tail."
    chunks = make_chunks(
        "paper-1",
        text,
        {"title": "fixture"},
        target_chars=80,
        overlap_chars=10,
        hard_max_chars=90,
        context_window=5,
    )
    formulas = [formula for chunk in chunks for formula in chunk.get("formulas", [])]
    ok = len(formulas) == 1 and "=1" in formulas[0]["latex"]
    return {"name": "chunk.formula_boundary", "ok": ok, "details": {"chunks": len(chunks), "formulas": len(formulas)}}


def check_turtle_parse() -> dict:
    chunks = [
        {
            "chunk_id": "paper-1::chunk::00000",
            "paper_id": "paper-1",
            "ordinal": 0,
            "text": "Energy relation $$E=mc^2$$.",
            "formulas": extract_formulas("Energy relation $$E=mc^2$$."),
            "metadata": {"title": "fixture", "pdf_sha256": "abc"},
        }
    ]
    turtle = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:test")
    try:
        Graph().parse(data=turtle, format="turtle")
        return {"name": "rdf.turtle_parse", "ok": True}
    except Exception as exc:  # noqa: BLE001
        return {"name": "rdf.turtle_parse", "ok": False, "details": str(exc)}



def check_no_unused_external_vector_db() -> dict:
    files = ["docker-compose.yml", ".env.example", "apps/api/src/main.rs", "apps/cli/src/main.rs", "scripts/bootstrap.sh"]
    hits = []
    for rel in files:
        text = (ROOT / rel).read_text(encoding="utf-8")
        if ("qd" + "rant") in text.lower():
            hits.append(rel)
    return {"name": "architecture.no_unused_external_vector_db", "ok": not hits, "details": hits}


def check_api_run_implemented() -> dict:
    text = (ROOT / "apps/api/src/main.rs").read_text(encoding="utf-8")
    required = ['route("/run"', "execute_autonomous_run", "run_command", "call_chat_model_json", "production_candidate_validated"]
    missing = [item for item in required if item not in text]
    return {"name": "api.autonomous_run", "ok": not missing, "details": {"missing": missing}}

def check_optional_command(name: str, args: list[str]) -> dict:
    try:
        result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, timeout=30, check=False)
        return {
            "name": name,
            "ok": result.returncode == 0,
            "details": {
                "returncode": result.returncode,
                "stdout": result.stdout[-1000:],
                "stderr": result.stderr[-1000:],
            },
        }
    except FileNotFoundError:
        return {"name": name, "ok": None, "details": "command unavailable in this environment"}
    except Exception as exc:  # noqa: BLE001
        return {"name": name, "ok": False, "details": str(exc)}


def main() -> int:
    checks = [check_file_exists(rel) for rel in REQUIRED_FILES]
    checks.extend([
        check_yaml(),
        check_formula_extraction(),
        check_chunk_formula_boundary(),
        check_turtle_parse(),
        check_no_unused_external_vector_db(),
        check_api_run_implemented(),
        check_optional_command("cargo.check", ["cargo", "check", "--workspace"]),
        check_optional_command("docker.compose.config", ["docker", "compose", "config"]),
    ])
    failed = [c for c in checks if c["ok"] is False]
    unavailable = [c for c in checks if c["ok"] is None]
    payload = {
        "ok": not failed,
        "summary": {
            "total": len(checks),
            "failed": len(failed),
            "unavailable": len(unavailable),
        },
        "checks": checks,
    }
    print(json.dumps(payload, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
