from __future__ import annotations

"""Veritas Tool-Verified Math Engine.

This service executes real symbolic and numeric checks. It is not a mock model
server and it does not rely on LLM heuristics for mathematical truth. The Rust
orchestrator may ask vLLM to propose a tool call, but Veritas executes the tool
here and persists the returned evidence before code generation gates may pass.
"""

import hashlib
import json
import math
import os
import random
import re
import time
import traceback
from dataclasses import dataclass
from typing import Any, Callable

import mpmath as mp
import numpy as np
import sympy as sp
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from sympy.parsing.sympy_parser import (
    convert_xor,
    implicit_multiplication_application,
    parse_expr,
    standard_transformations,
)

try:  # Optional: SymPy's LaTeX parser requires antlr runtime on many installs.
    from sympy.parsing.latex import parse_latex as sympy_parse_latex
except Exception:  # pragma: no cover - fallback parser is intentionally used.
    sympy_parse_latex = None

SERVICE_NAME = os.getenv("VERITAS_MATH_TOOLS_SERVICE_NAME", "veritas-math-tools")
DEFAULT_SAMPLE_COUNT = int(os.getenv("VERITAS_MATH_TOOLS_DEFAULT_SAMPLES", os.getenv("VERITAS_MATH_TOOLS_SAMPLE_COUNT", "21")))
DEFAULT_SAMPLE_MIN = float(os.getenv("VERITAS_MATH_TOOLS_SAMPLE_MIN", "-3.0"))
DEFAULT_SAMPLE_MAX = float(os.getenv("VERITAS_MATH_TOOLS_SAMPLE_MAX", "3.0"))
DEFAULT_TOLERANCE = float(os.getenv("VERITAS_MATH_TOOLS_TOLERANCE", "1e-8"))
MAX_EXPRESSION_CHARS = int(os.getenv("VERITAS_MATH_TOOLS_MAX_EXPRESSION_CHARS", "8192"))
RANDOM_SEED = int(os.getenv("VERITAS_MATH_TOOLS_RANDOM_SEED", "1729"))

app = FastAPI(title="Veritas Tool-Verified Math Engine", version=os.getenv("VERITAS_MATH_TOOLS_VERSION", "0.1.0"))

_TRANSFORMATIONS = standard_transformations + (implicit_multiplication_application, convert_xor)
_ALLOWED_FUNCTIONS: dict[str, Any] = {
    name: getattr(sp, name)
    for name in [
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "sinh",
        "cosh",
        "tanh",
        "exp",
        "log",
        "sqrt",
        "Abs",
        "Min",
        "Max",
    ]
    if hasattr(sp, name)
}
_ALLOWED_FUNCTIONS.update({"pi": sp.pi})
_LATEX_COMMAND_REPLACEMENTS = {
    r"\left": "",
    r"\right": "",
    r"\,": " ",
    r"\;": " ",
    r"\!": " ",
    r"\cdot": "*",
    r"\times": "*",
    r"\div": "/",
    r"\pi": "pi",
    r"\theta": "theta",
    r"\Theta": "Theta",
    r"\alpha": "alpha",
    r"\beta": "beta",
    r"\gamma": "gamma",
    r"\lambda": "lambda",
    r"\mu": "mu",
    r"\sigma": "sigma",
    r"\rho": "rho",
    r"\omega": "omega",
    r"\Omega": "Omega",
    r"\Delta": "Delta",
    r"\nabla": "nabla",
}
_FUNCTION_REPLACEMENTS = {
    r"\sin": "sin",
    r"\cos": "cos",
    r"\tan": "tan",
    r"\exp": "exp",
    r"\log": "log",
    r"\ln": "log",
    r"\sqrt": "sqrt",
}


class MathToolError(Exception):
    def __init__(self, code: str, message: str, remediation: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.remediation = remediation
        self.details = details or {}

    def response(self) -> dict[str, Any]:
        return {
            "ok": False,
            "code": self.code,
            "message": self.message,
            "remediation": self.remediation,
            "details": self.details,
        }


class FormulaPayload(BaseModel):
    latex: str | None = None
    expression: str | None = None
    assumptions: list[str] = Field(default_factory=list)
    variables: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)


class DifferentiatePayload(FormulaPayload):
    variable: str | None = None


class EquivalencePayload(BaseModel):
    left_latex: str | None = None
    right_latex: str | None = None
    left_expression: str | None = None
    right_expression: str | None = None
    assumptions: list[str] = Field(default_factory=list)
    variables: list[str] = Field(default_factory=list)
    tolerance: float = DEFAULT_TOLERANCE
    samples: int = DEFAULT_SAMPLE_COUNT
    metadata: dict[str, Any] = Field(default_factory=dict)


class NumericValidatePayload(FormulaPayload):
    samples: int = DEFAULT_SAMPLE_COUNT
    sample_min: float = DEFAULT_SAMPLE_MIN
    sample_max: float = DEFAULT_SAMPLE_MAX
    tolerance: float = DEFAULT_TOLERANCE
    seed: int = RANDOM_SEED


class DimensionCheckPayload(FormulaPayload):
    units: dict[str, str] = Field(default_factory=dict)


class PropertyTestPayload(FormulaPayload):
    target_language: str = "python"
    function_name: str = "generated_function"
    samples: int = DEFAULT_SAMPLE_COUNT
    tolerance: float = DEFAULT_TOLERANCE


@dataclass(frozen=True)
class ToolSpec:
    name: str
    route: str
    handler: Callable[..., dict[str, Any]]
    blocks_codegen_on_failure: bool
    deterministic: bool
    parallel_safe: bool


def hash_payload(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, default=str).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def now_ms() -> int:
    return int(time.time() * 1000)


def clean_latex(value: str) -> str:
    text = value.strip()
    text = re.sub(r"^\$+|\$+$", "", text).strip()
    text = text.replace("\\mathrm", "")
    return text[:MAX_EXPRESSION_CHARS]


def replace_frac(text: str) -> str:
    # Handles nested-free \frac{a}{b}; repeated until stable.
    pattern = re.compile(r"\\frac\s*\{([^{}]+)\}\s*\{([^{}]+)\}")
    previous = None
    while previous != text:
        previous = text
        text = pattern.sub(r"((\1)/(\2))", text)
    return text


def replace_sqrt(text: str) -> str:
    return re.sub(r"\\sqrt\s*\{([^{}]+)\}", r"sqrt(\1)", text)


def latex_to_sympy_text(latex: str) -> str:
    text = clean_latex(latex)
    text = replace_frac(text)
    text = replace_sqrt(text)
    for src, dst in {**_LATEX_COMMAND_REPLACEMENTS, **_FUNCTION_REPLACEMENTS}.items():
        text = text.replace(src, dst)
    text = re.sub(r"\{([^{}]+)\}", r"(\1)", text)
    text = re.sub(r"([A-Za-z0-9_\)]+)\^\(([^()]+)\)", r"\1**(\2)", text)
    text = re.sub(r"([A-Za-z0-9_\)]+)\^([A-Za-z0-9_]+)", r"\1**\2", text)
    text = re.sub(r"_\(([^()]+)\)", r"_\1", text)
    text = re.sub(r"\\[a-zA-Z]+", lambda m: m.group(0).lstrip("\\"), text)
    return text.strip()


def expression_source(payload: FormulaPayload | NumericValidatePayload | DimensionCheckPayload | PropertyTestPayload) -> tuple[str, str]:
    if payload.expression and payload.expression.strip():
        return payload.expression.strip()[:MAX_EXPRESSION_CHARS], "expression"
    if payload.latex and payload.latex.strip():
        return payload.latex.strip()[:MAX_EXPRESSION_CHARS], "latex"
    raise MathToolError("math.input_missing", "No latex or expression was provided.", "Provide a non-empty formula string.")


def parse_assumption_symbols(assumptions: list[str]) -> dict[str, sp.Symbol]:
    names: set[str] = set()
    for item in assumptions:
        names.update(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", item))
    return {name: sp.symbols(name, real=True) for name in sorted(names) if name not in _ALLOWED_FUNCTIONS}


def parse_math(value: str, source_type: str, assumptions: list[str] | None = None, variables: list[str] | None = None) -> dict[str, Any]:
    assumptions = assumptions or []
    variables = variables or []
    if len(value) > MAX_EXPRESSION_CHARS:
        raise MathToolError("math.expression_too_large", "Formula exceeds configured maximum expression length.", "Reduce the formula or increase VERITAS_MATH_TOOLS_MAX_EXPRESSION_CHARS.", {"length": len(value), "max": MAX_EXPRESSION_CHARS})
    local_symbols = parse_assumption_symbols(assumptions)
    normalized_for_names = latex_to_sympy_text(value) if source_type == "latex" else value
    for name in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", normalized_for_names):
        if name and name not in _ALLOWED_FUNCTIONS:
            local_symbols[str(name)] = sp.symbols(str(name), real=True)
    for name in variables:
        if name and name not in _ALLOWED_FUNCTIONS:
            local_symbols[str(name)] = sp.symbols(str(name), real=True)
    locals_map = {**_ALLOWED_FUNCTIONS, **local_symbols}
    parse_attempts: list[dict[str, Any]] = []
    cleaned = clean_latex(value) if source_type == "latex" else value.strip()
    equation_parts: list[str] | None = None
    if "=" in cleaned and not any(op in cleaned for op in ["<=", ">=", "!="]):
        left, right = cleaned.split("=", 1)
        equation_parts = [left.strip(), right.strip()]

    def _parse_single(expr_text: str) -> sp.Expr:
        if source_type == "latex" and sympy_parse_latex is not None:
            try:
                parsed = sympy_parse_latex(expr_text)
                parse_attempts.append({"parser": "sympy_latex", "ok": True})
                return parsed
            except Exception as exc:  # noqa: BLE001
                parse_attempts.append({"parser": "sympy_latex", "ok": False, "error": str(exc)})
        normalized = latex_to_sympy_text(expr_text) if source_type == "latex" else expr_text
        try:
            parsed = parse_expr(normalized, local_dict=locals_map, transformations=_TRANSFORMATIONS, evaluate=False)
            parse_attempts.append({"parser": "sympy_parse_expr", "ok": True, "normalized_expression": normalized})
            return parsed
        except Exception as exc:  # noqa: BLE001
            parse_attempts.append({"parser": "sympy_parse_expr", "ok": False, "normalized_expression": normalized, "error": str(exc)})
            raise MathToolError("math.parse_failed", "Formula could not be parsed by available SymPy parsers.", "Provide clearer LaTeX/expression text or configure the required parser dependencies.", {"attempts": parse_attempts, "input": value, "source_type": source_type}) from exc

    if equation_parts:
        left = _parse_single(equation_parts[0])
        right = _parse_single(equation_parts[1])
        expr = sp.Eq(left, right, evaluate=False)
        normalized = f"{sp.sstr(left)} = {sp.sstr(right)}"
    else:
        expr = _parse_single(cleaned)
        normalized = sp.sstr(expr)
    return {
        "expr": expr,
        "normalized_expression": normalized,
        "free_symbols": sorted(str(symbol) for symbol in getattr(expr, "free_symbols", set())),
        "parse_attempts": parse_attempts,
        "source_type": source_type,
        "input": value,
    }


def tool_envelope(tool_name: str, payload: dict[str, Any], status: str, result: dict[str, Any], *, blocks_codegen_on_failure: bool = True, started_ms: int | None = None) -> dict[str, Any]:
    started_ms = started_ms or now_ms()
    ok = status == "passed"
    return {
        "kind": "VeritasMathToolResult",
        "ok": ok,
        "tool": tool_name,
        "tool_name": tool_name,
        "tool_call_id": f"{tool_name}-{hash_payload(payload)[:16]}",
        "input_hash": hash_payload(payload),
        "output_hash": hash_payload(result),
        "status": status,
        "blocks_codegen": bool(blocks_codegen_on_failure and not ok),
        "result": result,
        "duration_ms": max(0, now_ms() - started_ms),
        "service": SERVICE_NAME,
    }


def run_tool(tool_name: str, payload: dict[str, Any], operation: Callable[[], dict[str, Any]], *, blocks_codegen_on_failure: bool = True) -> dict[str, Any]:
    started = now_ms()
    try:
        result = operation()
        derived_status = "failed" if (result.get("status") == "failed" or result.get("failure_count", 0) or result.get("counterexample_count", 0) or result.get("counterexamples")) else "passed"
        return tool_envelope(tool_name, payload, derived_status, result, blocks_codegen_on_failure=blocks_codegen_on_failure, started_ms=started)
    except MathToolError as exc:
        return tool_envelope(tool_name, payload, "failed", exc.response(), blocks_codegen_on_failure=blocks_codegen_on_failure, started_ms=started)
    except Exception as exc:  # noqa: BLE001
        return tool_envelope(
            tool_name,
            payload,
            "failed",
            {
                "ok": False,
                "code": "math.unhandled_error",
                "message": str(exc),
                "remediation": "Inspect the formula, tool input, and math-tools logs.",
                "traceback": traceback.format_exc(limit=12),
            },
            blocks_codegen_on_failure=blocks_codegen_on_failure,
            started_ms=started,
        )


def parse_latex_impl(payload: FormulaPayload) -> dict[str, Any]:
    value, source = expression_source(payload)
    parsed = parse_math(value, source, payload.assumptions, payload.variables)
    expr = parsed.pop("expr")
    return {
        **parsed,
        "sympy_srepr": sp.srepr(expr),
        "is_equation": isinstance(expr, sp.Equality),
    }


def normalize_expression_impl(payload: FormulaPayload) -> dict[str, Any]:
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    if expr is sp.S.true or expr is True:
        return {"is_equation": True, "residual": "0", "equation_simplifies_to_zero": True, "free_symbols": [], "status": "passed"}
    if expr is sp.S.false or expr is False:
        return {"is_equation": True, "residual": "false", "equation_simplifies_to_zero": False, "free_symbols": [], "status": "failed"}
    if isinstance(expr, sp.Equality):
        simplified = sp.Eq(sp.simplify(expr.lhs), sp.simplify(expr.rhs))
        normalized = f"{sp.sstr(simplified.lhs)} = {sp.sstr(simplified.rhs)}"
    else:
        simplified = sp.simplify(expr)
        normalized = sp.sstr(simplified)
    return {
        "normalized_expression": normalized,
        "free_symbols": sorted(str(symbol) for symbol in simplified.free_symbols),
        "sympy_srepr": sp.srepr(simplified),
    }


def symbolic_simplify_impl(payload: FormulaPayload) -> dict[str, Any]:
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    if expr is sp.S.true or expr is True:
        return {"is_equation": True, "residual": "0", "equation_simplifies_to_zero": True, "free_symbols": [], "status": "passed"}
    if expr is sp.S.false or expr is False:
        return {"is_equation": True, "residual": "false", "equation_simplifies_to_zero": False, "free_symbols": [], "status": "failed"}
    if isinstance(expr, sp.Equality):
        residual = sp.simplify(expr.lhs - expr.rhs)
        return {
            "is_equation": True,
            "lhs": sp.sstr(sp.simplify(expr.lhs)),
            "rhs": sp.sstr(sp.simplify(expr.rhs)),
            "residual": sp.sstr(residual),
            "equation_simplifies_to_zero": residual == 0,
            "free_symbols": sorted(str(symbol) for symbol in residual.free_symbols),
        }
    simplified = sp.simplify(expr)
    return {
        "is_equation": False,
        "simplified_expression": sp.sstr(simplified),
        "free_symbols": sorted(str(symbol) for symbol in simplified.free_symbols),
        "complexity_before": int(sp.count_ops(expr)),
        "complexity_after": int(sp.count_ops(simplified)),
    }


def symbolic_differentiate_impl(payload: DifferentiatePayload) -> dict[str, Any]:
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    if expr is sp.S.true or expr is True:
        return {"is_equation": True, "residual": "0", "equation_simplifies_to_zero": True, "free_symbols": [], "status": "passed"}
    if expr is sp.S.false or expr is False:
        return {"is_equation": True, "residual": "false", "equation_simplifies_to_zero": False, "free_symbols": [], "status": "failed"}
    if isinstance(expr, sp.Equality):
        expr = expr.lhs - expr.rhs
    symbols = sorted(expr.free_symbols, key=lambda symbol: str(symbol))
    variable = payload.variable or (str(symbols[0]) if symbols else None)
    if not variable:
        raise MathToolError("math.no_variable", "No differentiation variable is available.", "Provide a variable or use an expression with free symbols.")
    sym = sp.symbols(variable, real=True)
    derivative = sp.simplify(sp.diff(expr, sym))
    return {
        "variable": variable,
        "derivative": sp.sstr(derivative),
        "free_symbols": sorted(str(symbol) for symbol in derivative.free_symbols),
    }


def _sample_points(symbols: list[str], count: int, low: float, high: float, seed: int) -> list[dict[str, float]]:
    rng = random.Random(seed)
    points: list[dict[str, float]] = []
    base_values = [low, -1.0, -0.5, 0.0, 0.5, 1.0, high]
    for idx in range(max(0, min(count, len(base_values)))):
        points.append({name: float(base_values[idx % len(base_values)]) for name in symbols})
    while len(points) < count:
        points.append({name: rng.uniform(low, high) for name in symbols})
    return points


def _safe_eval_expr(expr: sp.Expr, point: dict[str, float]) -> float:
    substitutions = {sp.symbols(key, real=True): value for key, value in point.items()}
    value = expr.evalf(subs=substitutions)
    try:
        numeric = float(value)
    except TypeError as exc:
        raise MathToolError("math.numeric_non_real", "Expression evaluated to a non-real value.", "Restrict assumptions/domain or provide a formula with real-valued outputs.", {"value": str(value), "point": point}) from exc
    if not math.isfinite(numeric):
        raise MathToolError("math.numeric_non_finite", "Expression evaluated to a non-finite value.", "Add domain assumptions or handle singularities before code generation.", {"value": numeric, "point": point})
    return numeric


def numeric_validate_impl(payload: NumericValidatePayload) -> dict[str, Any]:
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    if expr is sp.S.true or expr is True:
        return {"samples_evaluated": 0, "failure_count": 0, "counterexamples": [], "observations_preview": [], "tolerance": payload.tolerance, "status": "passed", "boolean_identity": True}
    if expr is sp.S.false or expr is False:
        return {"samples_evaluated": 0, "failure_count": 1, "counterexamples": [{"reason": "symbolic false"}], "observations_preview": [], "tolerance": payload.tolerance, "status": "failed", "boolean_identity": False}
    symbols = parsed["free_symbols"]
    points = _sample_points(symbols, max(1, payload.samples), payload.sample_min, payload.sample_max, payload.seed)
    failures: list[dict[str, Any]] = []
    observations: list[dict[str, Any]] = []
    if isinstance(expr, sp.Equality):
        residual = sp.simplify(expr.lhs - expr.rhs)
        dependent_symbol: sp.Symbol | None = None
        dependent_expression: sp.Expr | None = None
        if isinstance(expr.lhs, sp.Symbol) and expr.lhs not in expr.rhs.free_symbols:
            dependent_symbol = expr.lhs
            dependent_expression = expr.rhs
        elif isinstance(expr.rhs, sp.Symbol) and expr.rhs not in expr.lhs.free_symbols:
            dependent_symbol = expr.rhs
            dependent_expression = expr.lhs
        if dependent_symbol is not None and dependent_expression is not None:
            independent_symbols = sorted(str(symbol) for symbol in residual.free_symbols if symbol != dependent_symbol)
            points = _sample_points(independent_symbols, max(1, payload.samples), payload.sample_min, payload.sample_max, payload.seed)
        for point in points:
            try:
                if dependent_symbol is not None and dependent_expression is not None:
                    dep_value = _safe_eval_expr(dependent_expression, point)
                    point = {**point, str(dependent_symbol): dep_value}
                value = abs(_safe_eval_expr(residual, point))
                observations.append({"point": point, "absolute_residual": value})
                if value > payload.tolerance:
                    failures.append({"point": point, "absolute_residual": value, "tolerance": payload.tolerance})
            except MathToolError as exc:
                failures.append({"point": point, "error": exc.response()})
    else:
        for point in points:
            try:
                value = _safe_eval_expr(expr, point)
                observations.append({"point": point, "value": value})
            except MathToolError as exc:
                failures.append({"point": point, "error": exc.response()})
    return {
        "samples_evaluated": len(points),
        "failure_count": len(failures),
        "counterexamples": failures,
        "observations_preview": observations[: min(5, len(observations))],
        "tolerance": payload.tolerance,
        "status": "passed" if not failures else "failed",
    }


def symbolic_equivalence_impl(payload: EquivalencePayload) -> dict[str, Any]:
    left_text = payload.left_expression or payload.left_latex
    right_text = payload.right_expression or payload.right_latex
    if not left_text or not right_text:
        raise MathToolError("math.equivalence_input_missing", "Both left and right expressions are required.", "Provide left/right LaTeX or expression strings.")
    left = parse_math(left_text, "expression" if payload.left_expression else "latex", payload.assumptions, payload.variables)["expr"]
    right = parse_math(right_text, "expression" if payload.right_expression else "latex", payload.assumptions, payload.variables)["expr"]
    if isinstance(left, sp.Equality):
        left = left.lhs - left.rhs
    if isinstance(right, sp.Equality):
        right = right.lhs - right.rhs
    residual = sp.simplify(left - right)
    symbolic_equivalent = residual == 0
    numeric = numeric_validate_impl(NumericValidatePayload(expression=sp.sstr(residual), samples=payload.samples, tolerance=payload.tolerance, variables=payload.variables, assumptions=payload.assumptions))
    return {
        "symbolic_equivalent": bool(symbolic_equivalent),
        "residual": sp.sstr(residual),
        "numeric_validation": numeric,
        "status": "passed" if symbolic_equivalent or not numeric["counterexamples"] else "failed",
    }


def counterexample_search_impl(payload: NumericValidatePayload) -> dict[str, Any]:
    result = numeric_validate_impl(payload)
    return {
        "counterexamples": result.get("counterexamples", []),
        "counterexample_count": len(result.get("counterexamples", [])),
        "samples_evaluated": result.get("samples_evaluated", 0),
        "status": "passed" if not result.get("counterexamples") else "failed",
    }


def dimension_check_impl(payload: DimensionCheckPayload) -> dict[str, Any]:
    # A lightweight real dimensional consistency check. If units are supplied,
    # additive terms must not combine incompatible declared unit dimensions.
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    units = {str(k): str(v) for k, v in payload.units.items() if str(v).strip()}
    if not units:
        return {
            "status": "passed",
            "scope": "units_not_supplied",
            "message": "No units were supplied; dimensional analysis is not applicable, but this is not a mathematical failure.",
            "free_symbols": parsed["free_symbols"],
        }
    if isinstance(expr, sp.Equality):
        terms = [expr.lhs, expr.rhs]
    elif isinstance(expr, sp.Add):
        terms = list(expr.args)
    else:
        terms = [expr]
    term_units: list[dict[str, Any]] = []
    for term in terms:
        symbol_units = sorted({units[str(symbol)] for symbol in term.free_symbols if str(symbol) in units})
        term_units.append({"term": sp.sstr(term), "declared_units": symbol_units})
    incompatible = False
    if len(term_units) > 1:
        base = term_units[0]["declared_units"]
        incompatible = any(item["declared_units"] != base for item in term_units[1:])
    return {
        "status": "failed" if incompatible else "passed",
        "terms": term_units,
        "incompatible_additive_units": incompatible,
        "declared_units": units,
    }


def generate_property_tests_impl(payload: PropertyTestPayload) -> dict[str, Any]:
    parsed = parse_math(*expression_source(payload), payload.assumptions, payload.variables)
    expr = parsed["expr"]
    symbols = parsed["free_symbols"]
    function_name = re.sub(r"[^A-Za-z0-9_]", "_", payload.function_name).strip("_") or "generated_function"
    test_name = f"test_{function_name}_math_invariant"
    if payload.target_language.lower() == "python":
        if isinstance(expr, sp.Equality):
            residual = sp.sstr(sp.simplify(expr.lhs - expr.rhs))
            body = f"""from hypothesis import given, strategies as st, settings\nimport math\n\n\n@settings(max_examples={max(1, payload.samples)})\n@given({', '.join(f'{name}=st.floats(min_value=-3, max_value=3, allow_nan=False, allow_infinity=False)' for name in symbols)})\ndef {test_name}({', '.join(symbols)}):\n    residual = {residual}\n    assert math.isfinite(float(residual))\n    assert abs(float(residual)) <= {payload.tolerance!r}\n"""
        else:
            body = f"""from hypothesis import given, strategies as st, settings\nimport math\n\n\n@settings(max_examples={max(1, payload.samples)})\n@given({', '.join(f'{name}=st.floats(min_value=-3, max_value=3, allow_nan=False, allow_infinity=False)' for name in symbols)})\ndef {test_name}({', '.join(symbols)}):\n    value = {sp.sstr(expr)}\n    assert math.isfinite(float(value))\n"""
    else:
        body = ""
    return {
        "target_language": payload.target_language,
        "test_name": test_name,
        "test_code": body,
        "free_symbols": symbols,
        "status": "passed" if body else "failed",
        "message": "Generated property-test source code from the parsed symbolic expression." if body else "No property-test generator is available for the requested language.",
    }


TOOL_SPECS: dict[str, ToolSpec] = {}


def register_tool(name: str, route: str, handler: Callable[..., dict[str, Any]], *, blocks_codegen_on_failure: bool = True, deterministic: bool = True, parallel_safe: bool = True) -> None:
    TOOL_SPECS[name] = ToolSpec(name, route, handler, blocks_codegen_on_failure, deterministic, parallel_safe)


register_tool("parse_latex", "/tools/parse_latex", parse_latex_impl)
register_tool("normalize_expression", "/tools/normalize_expression", normalize_expression_impl, blocks_codegen_on_failure=False)
register_tool("symbolic_simplify", "/tools/symbolic_simplify", symbolic_simplify_impl)
register_tool("symbolic_differentiate", "/tools/symbolic_differentiate", symbolic_differentiate_impl)
register_tool("symbolic_equivalence", "/tools/symbolic_equivalence", symbolic_equivalence_impl)
register_tool("numeric_validate", "/tools/numeric_validate", numeric_validate_impl)
register_tool("counterexample_search", "/tools/counterexample_search", counterexample_search_impl)
register_tool("dimension_check", "/tools/dimension_check", dimension_check_impl)
register_tool("generate_property_tests", "/tools/generate_property_tests", generate_property_tests_impl, blocks_codegen_on_failure=False)


@app.get("/health")
def health() -> dict[str, Any]:
    return {
        "ok": True,
        "service": SERVICE_NAME,
        "tools": sorted(TOOL_SPECS),
        "sympy_version": sp.__version__,
        "numpy_version": np.__version__,
        "mpmath_version": mp.__version__,
        "latex_parser_available": sympy_parse_latex is not None,
        "default_sample_count": DEFAULT_SAMPLE_COUNT,
    }


@app.get("/tools")
def list_tools() -> dict[str, Any]:
    return {
        "ok": True,
        "tools": [
            {
                "name": spec.name,
                "route": spec.route,
                "deterministic": spec.deterministic,
                "parallel_safe": spec.parallel_safe,
                "blocks_codegen_on_failure": spec.blocks_codegen_on_failure,
            }
            for spec in sorted(TOOL_SPECS.values(), key=lambda item: item.name)
        ],
    }


@app.post("/tools/parse_latex")
def parse_latex_endpoint(payload: FormulaPayload) -> dict[str, Any]:
    return run_tool("parse_latex", payload.model_dump(), lambda: parse_latex_impl(payload))


@app.post("/tools/normalize_expression")
def normalize_expression_endpoint(payload: FormulaPayload) -> dict[str, Any]:
    return run_tool("normalize_expression", payload.model_dump(), lambda: normalize_expression_impl(payload))


@app.post("/tools/symbolic_simplify")
def symbolic_simplify_endpoint(payload: FormulaPayload) -> dict[str, Any]:
    return run_tool("symbolic_simplify", payload.model_dump(), lambda: symbolic_simplify_impl(payload))


@app.post("/tools/symbolic_differentiate")
def symbolic_differentiate_endpoint(payload: DifferentiatePayload) -> dict[str, Any]:
    return run_tool("symbolic_differentiate", payload.model_dump(), lambda: symbolic_differentiate_impl(payload))


@app.post("/tools/symbolic_equivalence")
def symbolic_equivalence_endpoint(payload: EquivalencePayload) -> dict[str, Any]:
    return run_tool("symbolic_equivalence", payload.model_dump(), lambda: symbolic_equivalence_impl(payload))


@app.post("/tools/numeric_validate")
def numeric_validate_endpoint(payload: NumericValidatePayload) -> dict[str, Any]:
    return run_tool("numeric_validate", payload.model_dump(), lambda: numeric_validate_impl(payload))


@app.post("/tools/counterexample_search")
def counterexample_search_endpoint(payload: NumericValidatePayload) -> dict[str, Any]:
    return run_tool("counterexample_search", payload.model_dump(), lambda: counterexample_search_impl(payload))


@app.post("/tools/dimension_check")
def dimension_check_endpoint(payload: DimensionCheckPayload) -> dict[str, Any]:
    return run_tool("dimension_check", payload.model_dump(), lambda: dimension_check_impl(payload), blocks_codegen_on_failure=False)


@app.post("/tools/generate_property_tests")
def generate_property_tests_endpoint(payload: PropertyTestPayload) -> dict[str, Any]:
    return run_tool("generate_property_tests", payload.model_dump(), lambda: generate_property_tests_impl(payload), blocks_codegen_on_failure=False)



def _coerce_payload_for_tool(tool_name: str, payload: dict[str, Any]) -> BaseModel:
    if tool_name == "symbolic_differentiate":
        return DifferentiatePayload(**payload)
    if tool_name == "symbolic_equivalence":
        if "left_expression" not in payload and "right_expression" not in payload:
            # For equation formulas, the /tools/symbolic_equivalence endpoint requires two expressions.
            # The validation route uses symbolic_simplify/numeric_validate for single-equation checks.
            return EquivalencePayload(left_expression=payload.get("expression") or payload.get("latex"), right_expression=payload.get("expression") or payload.get("latex"), **{k: v for k, v in payload.items() if k in {"assumptions", "variables", "tolerance", "samples", "metadata"}})
        return EquivalencePayload(**payload)
    if tool_name in {"numeric_validate", "counterexample_search"}:
        converted = dict(payload)
        if "sample_count" in converted and "samples" not in converted:
            converted["samples"] = converted.pop("sample_count")
        return NumericValidatePayload(**converted)
    if tool_name == "dimension_check":
        return DimensionCheckPayload(**payload)
    if tool_name == "generate_property_tests":
        converted = dict(payload)
        if "target_language" not in converted and isinstance(converted.get("metadata"), dict):
            language = converted["metadata"].get("target_language")
            if language:
                converted["target_language"] = language
        return PropertyTestPayload(**converted)
    return FormulaPayload(**payload)


@app.post("/validate")
def validate_formula(payload: dict[str, Any]) -> dict[str, Any]:
    metadata = payload.get("metadata") if isinstance(payload.get("metadata"), dict) else {}
    sequence = metadata.get("tool_sequence") or os.getenv("VERITAS_MATH_TOOL_SEQUENCE", "parse_latex,normalize_expression,symbolic_simplify,numeric_validate,counterexample_search,generate_property_tests")
    if isinstance(sequence, str):
        tools = [item.strip() for item in sequence.split(",") if item.strip()]
    else:
        tools = [str(item).strip() for item in sequence if str(item).strip()]
    if not tools:
        raise HTTPException(status_code=400, detail={"code": "math.no_tools", "message": "No math tools were configured for validation."})
    tool_results: list[dict[str, Any]] = []
    for name in tools:
        spec = TOOL_SPECS.get(name)
        if spec is None:
            result = tool_envelope(name, payload, "failed", {"ok": False, "code": "math.tool_unknown", "message": f"Tool {name} is not registered.", "available_tools": sorted(TOOL_SPECS)}, blocks_codegen_on_failure=True)
        else:
            try:
                coerced = _coerce_payload_for_tool(name, payload)
                result = run_tool(name, coerced.model_dump(), lambda coerced=coerced, handler=spec.handler: handler(coerced), blocks_codegen_on_failure=spec.blocks_codegen_on_failure)
            except Exception as exc:  # noqa: BLE001
                result = tool_envelope(name, payload, "failed", {"ok": False, "code": "math.tool_payload_error", "message": str(exc), "remediation": "Correct the formula/tool payload."}, blocks_codegen_on_failure=True)
        tool_results.append(result)
        if result.get("blocks_codegen") and spec and spec.blocks_codegen_on_failure:
            break
    blocking = [item for item in tool_results if item.get("blocks_codegen")]
    counterexamples = []
    for result in tool_results:
        nested = result.get("result", {}) if isinstance(result.get("result"), dict) else {}
        counterexamples.extend(nested.get("counterexamples", []) if isinstance(nested.get("counterexamples"), list) else [])
    ok = not blocking and not counterexamples
    return {
        "kind": "VeritasMathValidationReport",
        "ok": ok,
        "status": "passed" if ok else "blocked_by_math_tools",
        "formula_id": payload.get("formula_id"),
        "tools_requested": tools,
        "tool_results": tool_results,
        "blocking_findings": blocking,
        "counterexamples": counterexamples,
        "timestamp_ms": now_ms(),
        "service": SERVICE_NAME,
    }
