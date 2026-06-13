from __future__ import annotations

import json
import os
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI(title="Veritas fake vLLM OpenAI-compatible server")


class ChatRequest(BaseModel):
    model: str | None = None
    messages: list[dict] = []
    temperature: float | None = None
    top_p: float | None = None
    max_tokens: int | None = None
    response_format: dict | None = None
    guided_json: dict | None = None


@app.get("/health")
def health():
    return {"ok": True, "service": "fake-vllm"}


@app.get("/v1/models")
def models():
    model = os.getenv("FAKE_VLLM_MODEL", "veritas-fake-model")
    extra = [m.strip() for m in os.getenv("FAKE_VLLM_EXTRA_MODELS", "").split(",") if m.strip()]
    return {"object": "list", "data": [{"id": model, "object": "model"}, *[{"id": m, "object": "model"} for m in extra]]}


@app.post("/v1/chat/completions")
def chat(req: ChatRequest):
    mode = os.getenv("FAKE_VLLM_RESPONSE_MODE", os.getenv("FAKE_VLLM_MODE", "valid")).lower()
    if mode == "invalid_json":
        payload = "not valid json"
    elif mode == "schema_violation":
        payload = json.dumps({"objective": {"summary": "missing required arrays"}})
    elif mode == "unsafe_path":
        payload = json.dumps({"package_name": "bad", "language": "rust", "files": [{"path": "../evil.rs", "content": "bad"}], "commands": []})
    elif mode == "unknown_tool":
        payload = json.dumps({"objective": {"summary": "bad"}, "steps": [{"id": "bad", "tool": "delete_world", "description": "unsafe", "input": {}, "success_criteria": ["bad"]}], "risks": [], "validation_gates": []})
    else:
        payload = json.dumps(_content_for_request(req))
    return {"id": "fake", "object": "chat.completion", "choices": [{"index": 0, "message": {"role": "assistant", "content": payload}, "finish_reason": "stop"}]}


def _content_for_request(req: ChatRequest) -> dict:
    text = "\n".join(str(m.get("content", "")) for m in req.messages)
    if "Veritas Code Writer" in text or "code" in (req.model or "").lower():
        return {
            "package_name": "veritas_generated_example",
            "language": "rust",
            "files": [
                {"path": "Cargo.toml", "content": "[package]\nname = \"veritas_generated_example\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n", "purpose": "Rust package manifest", "derived_from_plan_step_ids": ["code"], "derived_from_evidence_ids": ["chunk-1"], "derived_from_citation_ids": ["citation-1"], "derived_from_formula_ids": ["formula-1"], "required_validation_ids": ["vg-cargo-test"]},
                {"path": "src/lib.rs", "content": "pub fn add(a: i64, b: i64) -> i64 { a + b }\n\n#[cfg(test)]\nmod tests { use super::*; #[test] fn test_add() { assert_eq!(add(2, 3), 5); } }\n", "purpose": "Implementation and unit test for formula-backed operation", "derived_from_plan_step_ids": ["code", "test"], "derived_from_evidence_ids": ["chunk-1"], "derived_from_citation_ids": ["citation-1"], "derived_from_formula_ids": ["formula-1"], "required_validation_ids": ["vg-cargo-test"]},
            ],
            "commands": [{"command": "cargo test", "purpose": "compile and execute generated unit tests", "derived_from_plan_step_ids": ["test"], "required_validation_ids": ["vg-cargo-test"]}],
            "assumptions": ["fake vLLM response used for E2E orchestration validation"],
            "validation_summary": "cargo test must pass before production_candidate_validated",
            "artifact_status": "generated_unvalidated",
        }
    if "Math Reasoner" in text or "math" in (req.model or "").lower():
        return {
            "summary": "The supplied expression is treated as a symbolic shadow, not as mathematical truth by itself.",
            "axiom_map": ["A3 equations are constraint shadows", "A4 invariants are the true objects", "A8 symbols are subordinate", "A9 epistemic legitimacy"],
            "surface_phenomenon": {"description": "A formula-like expression supplied for implementation", "why_surface_may_mislead": "Variable domains and transformation constraints are underspecified"},
            "representation_hypothesis": "Represent the formula as a pure function over explicit typed inputs with separate validation of assumptions.",
            "candidate_representation_map": {"map": "R: formula text -> typed functional representation", "preserves": ["deterministic semantics"], "discards": ["layout artifacts"], "map_type": "loss-aware symbolic normalization"},
            "primitive_ontology": [{"entity": "input", "status": "explicit argument"}, {"entity": "output", "status": "computed value"}],
            "transformation_space": [{"transformation": "repeated evaluation with same input", "preserves_identity": True}],
            "constraint_geometry": [{"constraint": "implementation must not introduce hidden state"}],
            "invariants": [{"name": "referential transparency", "transformation_family": "same input repeated", "status": "candidate"}],
            "compression_fidelity": {"preserved": ["formula structure"], "discarded": ["PDF layout"], "risk": "domain assumptions may still be missing"},
            "recursive_closure": {"behavior": "function composition should remain deterministic", "termination": "finite expression evaluation"},
            "generative_necessity": [{"claim": "tests are required before production status", "status": "engineering necessity"}],
            "symbolic_shadows": [{"expression": "provided formula", "scope": "implementation clue", "failure_conditions": ["missing domain", "bad numerical tolerance"]}],
            "transfer_tests": [{"case": "edge numeric inputs", "expected": "finite deterministic output"}],
            "risks": [{"risk": "formula semantics under-specified", "severity": "high", "mitigation": "human formula review and unit tests"}],
            "validation_requirements": ["compile", "unit tests", "edge cases", "invariant assertions"],
            "status": "plausible",
        }
    return {
        "objective": {"summary": "Generate a tested implementation from evidence", "desired_outcome": "validated build artifact"},
        "steps": [
            {"id": "retrieve", "tool": "retrieval", "description": "Retrieve evidence", "input": {"query": "generated implementation"}, "success_criteria": ["evidence returned"], "evidence_ids": ["chunk-1"], "citation_ids": ["citation-1"], "formula_ids": ["formula-1"], "risk_ids": ["risk-model-output"], "risk_ids": ["risk-1"], "validation_gate_ids": ["vg-cargo-test"], "human_checkpoint_ids": ["plan_review"]},
            {"id": "math", "tool": "math_reasoning", "description": "Identify symbolic-shadow assumptions and invariants", "input": {"goal": "analyze"}, "success_criteria": ["invariants listed"], "evidence_ids": ["chunk-1"], "citation_ids": ["citation-1"], "formula_ids": ["formula-1"], "risk_ids": ["risk-model-output"], "risk_ids": ["risk-1"], "validation_gate_ids": ["vg-cargo-test"], "human_checkpoint_ids": ["plan_review"]},
            {"id": "code", "tool": "code_generation", "description": "Write implementation files", "input": {}, "success_criteria": ["files written"], "evidence_ids": ["chunk-1"], "citation_ids": ["citation-1"], "formula_ids": ["formula-1"], "risk_ids": ["risk-model-output"], "risk_ids": ["risk-1"], "validation_gate_ids": ["vg-cargo-test"], "human_checkpoint_ids": ["code_architecture_review"]},
            {"id": "test", "tool": "test_runner", "description": "Run validation commands", "input": {}, "success_criteria": ["tests pass"], "evidence_ids": ["chunk-1"], "citation_ids": ["citation-1"], "formula_ids": ["formula-1"], "risk_ids": ["risk-model-output"], "risk_ids": ["risk-1"], "validation_gate_ids": ["vg-cargo-test"], "human_checkpoint_ids": ["validation_review"]},
        ],
        "files_to_generate": [{"path": "src/lib.rs", "purpose": "implementation", "derived_from_step_ids": ["code"]}],
        "commands_to_run": [{"command": "cargo test", "purpose": "validate generated package", "validation_gate_ids": ["vg-cargo-test"]}],
        "risks": [{"id": "risk-model-output", "risk": "model output may be incomplete", "mitigation": "schema validation and tests", "severity": "medium"}],
        "validation_gates": [{"id": "vg-cargo-test", "check": "cargo test", "command": "cargo test", "expected_result": "all tests pass"}],
    }
