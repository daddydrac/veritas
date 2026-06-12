"""Source/mocked SHACL governance contracts for Veritas Phase 5.

These helpers intentionally do not require a live SHACL service. They model the
closed-world readiness checks Veritas must enforce before math-heavy research is
allowed to become production-grade code.
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from rdflib import Graph, Literal, Namespace, RDF, URIRef

VERITAS = Namespace("https://github.com/daddydrac/veritas/ontology#")

PHASE5_REQUIRED_SHAPE_NAMES = {
    "SymbolicShadowMathReadinessShape",
    "SymbolicShadowExtractionReadinessShape",
    "RepresentationMapMathShape",
    "InvariantMathShape",
    "GenerativeNecessityMathShape",
    "MathematicalDiscoveryArtifactReadinessShape",
    "MathToCodeValidationRequirementShape",
    "ProductionBuildArtifactValidationShape",
    "ShaclFindingShape",
}


@dataclass(frozen=True)
class GovernanceFinding:
    focus_node: str
    rule: str
    message: str
    severity: str = "Violation"

    def as_dict(self) -> dict[str, str]:
        return {
            "focus_node": self.focus_node,
            "rule": self.rule,
            "message": self.message,
            "severity": self.severity,
        }


def load_combined_shape_pack(root: Path | None = None) -> str:
    """Return the automatic Veritas SHACL rule pack: core + math."""

    if root is None:
        root = Path(__file__).resolve().parents[3]
    core = root / "packages/ontology/shacl/veritas-core.shacl.ttl"
    math = root / "packages/ontology/shacl/veritas-math.shacl.ttl"
    return core.read_text(encoding="utf-8") + "\n\n" + math.read_text(encoding="utf-8")


def shape_pack_contract(shapes_ttl: str) -> dict[str, Any]:
    """Parse and inspect the combined SHACL pack without pySHACL."""

    graph = Graph().parse(data=shapes_ttl, format="turtle")
    shape_names = {str(subject).split("#")[-1] for subject in graph.subjects(RDF.type, URIRef("http://www.w3.org/ns/shacl#NodeShape"))}
    missing = sorted(PHASE5_REQUIRED_SHAPE_NAMES - shape_names)
    return {
        "ok": not missing,
        "shape_count": len(shape_names),
        "missing": missing,
        "has_core_shapes": "ResearchObjectiveShape" in shape_names and "PlanShape" in shape_names,
        "has_math_shapes": "SymbolicShadowMathReadinessShape" in shape_names and "MathToCodeValidationRequirementShape" in shape_names,
    }


def _objects(graph: Graph, subject: URIRef, predicate: URIRef) -> list[Any]:
    return list(graph.objects(subject, predicate))


def _has_any(graph: Graph, subject: URIRef, predicate: URIRef) -> bool:
    return any(True for _ in graph.objects(subject, predicate))


def _literal_values(graph: Graph, subject: URIRef, predicate: URIRef) -> list[str]:
    return [str(value) for value in graph.objects(subject, predicate) if isinstance(value, Literal)]


def _add_if_missing(findings: list[GovernanceFinding], graph: Graph, subject: URIRef, predicate: URIRef, rule: str, message: str) -> None:
    if not _has_any(graph, subject, predicate):
        findings.append(GovernanceFinding(str(subject), rule, message))


def validate_math_governance_contract(data_ttl: str) -> dict[str, Any]:
    """A deterministic source-level analogue of the Phase 5 SHACL gate.

    This is not a replacement for pySHACL in production. It allows CI and this
    sandbox to verify that the same obligations are encoded in fixtures and
    control-plane behavior without requiring a live SHACL container.
    """

    graph = Graph().parse(data=data_ttl, format="turtle")
    findings: list[GovernanceFinding] = []

    for subject in graph.subjects(RDF.type, VERITAS.SymbolicShadow):
        for predicate, rule, message in [
            (VERITAS.derivedFrom, "symbolic_shadow.evidence", "SymbolicShadow must derive from evidence."),
            (VERITAS.hasExpressionText, "symbolic_shadow.expression", "SymbolicShadow must preserve expression text."),
            (VERITAS.hasNormalizedExpressionText, "symbolic_shadow.normalized_expression", "SymbolicShadow must preserve normalized expression text."),
            (VERITAS.hasFormulaSource, "symbolic_shadow.formula_source", "SymbolicShadow must record formula source."),
            (VERITAS.hasFormulaImageStatus, "symbolic_shadow.image_status", "SymbolicShadow must record formula image status."),
            (VERITAS.hasLatexOcrStatus, "symbolic_shadow.ocr_status", "SymbolicShadow must record LaTeX OCR status."),
            (VERITAS.hasHumanValidationStatus, "symbolic_shadow.human_review", "SymbolicShadow must record human validation status."),
            (VERITAS.hasConfidenceValue, "symbolic_shadow.confidence", "SymbolicShadow must record extraction confidence."),
        ]:
            _add_if_missing(findings, graph, subject, predicate, rule, message)

    for subject in graph.subjects(RDF.type, VERITAS.RepresentationMap):
        for predicate, rule, message in [
            (VERITAS.mapsFromSurface, "representation.surface", "RepresentationMap must map from surface phenomenon."),
            (VERITAS.mapsToLatentStructure, "representation.latent", "RepresentationMap must map to latent structure."),
            (VERITAS.preservesInvariant, "representation.invariant", "RepresentationMap must state preserved invariant."),
        ]:
            _add_if_missing(findings, graph, subject, predicate, rule, message)

    for subject in graph.subjects(RDF.type, VERITAS.Invariant):
        _add_if_missing(findings, graph, subject, VERITAS.hasTransformationFamily, "invariant.transformation_family", "Invariant must specify transformation family.")

    for subject in graph.subjects(RDF.type, VERITAS.GenerativeNecessityClaim):
        for predicate, rule, message in [
            (VERITAS.supportedByEvidence, "necessity.evidence", "GenerativeNecessityClaim must be evidence-backed."),
            (VERITAS.hasProofStatus, "necessity.proof_status", "GenerativeNecessityClaim must disclose proof status."),
            (VERITAS.testedByTransferTest, "necessity.transfer", "GenerativeNecessityClaim must include transfer test."),
        ]:
            _add_if_missing(findings, graph, subject, predicate, rule, message)

    for subject in graph.subjects(RDF.type, VERITAS.MathematicalDiscoveryArtifact):
        for predicate, rule, message in [
            (VERITAS.hasAxiomMap, "math.axiom_map", "MathematicalDiscoveryArtifact must include an Axiom Map."),
            (VERITAS.hasRepresentationMap, "math.representation_map", "MathematicalDiscoveryArtifact must include a representation map."),
            (VERITAS.hasValidationRequirement, "math.validation_requirement", "MathematicalDiscoveryArtifact must specify validation requirements."),
        ]:
            _add_if_missing(findings, graph, subject, predicate, rule, message)
        status_values = set(_literal_values(graph, subject, VERITAS.hasStatus))
        if not _has_any(graph, subject, VERITAS.hasInvariant) and not (status_values & {"exploratory", "speculative"}):
            findings.append(GovernanceFinding(str(subject), "math.invariant_or_status", "MathematicalDiscoveryArtifact must identify invariants or be explicitly exploratory/speculative."))

    for subject in graph.subjects(RDF.type, VERITAS.SourceCodeArtifact):
        _add_if_missing(findings, graph, subject, VERITAS.validatedBy, "source.validation", "SourceCodeArtifact must have validation checks.")
        _add_if_missing(findings, graph, subject, VERITAS.testedBy, "source.tests", "SourceCodeArtifact must be connected to tests.")

    for subject in graph.subjects(RDF.type, VERITAS.BuildArtifact):
        _add_if_missing(findings, graph, subject, VERITAS.hasStatus, "build.status", "BuildArtifact must have explicit status.")
        if "production_candidate_validated" in _literal_values(graph, subject, VERITAS.hasStatus):
            _add_if_missing(findings, graph, subject, VERITAS.validatedBy, "build.production_validation", "Validated BuildArtifact must link to validation result.")

    for subject in graph.subjects(RDF.type, VERITAS.LoopSpecification):
        _add_if_missing(findings, graph, subject, VERITAS.hasTerminationCondition, "loop.termination", "LoopSpecification must have a termination condition.")

    return {
        "ok": not findings,
        "conforms": not findings,
        "finding_count": len(findings),
        "findings": [finding.as_dict() for finding in findings],
    }


def shacl_findings_to_turtle(run_id: str, findings: Iterable[dict[str, Any]]) -> str:
    findings_list = list(findings)
    graph = Graph()
    graph.bind("veritas", VERITAS)
    run = URIRef(f"urn:veritas:run:{run_id}")
    for idx, finding in enumerate(findings_list):
        node = URIRef(f"urn:veritas:shacl-finding:{run_id}:{idx}")
        graph.add((node, RDF.type, VERITAS.Finding))
        graph.add((node, VERITAS.derivedFrom, run))
        graph.add((node, VERITAS.hasStatus, Literal("open")))
        graph.add((node, VERITAS.hasDescription, Literal(str(finding.get("message", "SHACL governance finding")))))
        graph.add((node, VERITAS.hasIdentifier, Literal(str(finding.get("rule", f"finding-{idx}")))))
    if not findings_list:
        node = URIRef(f"urn:veritas:shacl-finding:{run_id}:conforms")
        graph.add((node, RDF.type, VERITAS.Finding))
        graph.add((node, VERITAS.derivedFrom, run))
        graph.add((node, VERITAS.hasStatus, Literal("closed")))
        graph.add((node, VERITAS.hasDescription, Literal("SHACL governance validation conformed.")))
    return graph.serialize(format="turtle")


def complete_math_to_code_ttl() -> str:
    return """@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<urn:doc:paper> a veritas:SourceDocument .
<urn:chunk:1> a veritas:RetrievalResult ; veritas:derivedFrom <urn:doc:paper> .
<urn:evidence:1> a veritas:EvidenceArtifact .
<urn:surface:1> a veritas:SurfacePhenomenonDescription .
<urn:latent:1> a veritas:LatentStructureDescription .
<urn:tf:1> a veritas:TransformationFamily .
<urn:invariant:1> a veritas:Invariant ; veritas:hasTransformationFamily <urn:tf:1> .
<urn:repr:1> a veritas:RepresentationMap ;
  veritas:mapsFromSurface <urn:surface:1> ;
  veritas:mapsToLatentStructure <urn:latent:1> ;
  veritas:preservesInvariant <urn:invariant:1> .
<urn:formula:1> a veritas:SymbolicShadow ;
  veritas:derivedFrom <urn:chunk:1> ;
  veritas:hasExpressionText "L(\\theta)=E_q[log p-log q]" ;
  veritas:hasNormalizedExpressionText "L(\\theta)=E_q[log p-log q]" ;
  veritas:hasFormulaSource "docling+ocr" ;
  veritas:hasFormulaImageStatus "rendered" ;
  veritas:hasLatexOcrStatus "accepted" ;
  veritas:hasHumanValidationStatus "approved" ;
  veritas:hasConfidenceValue "0.97"^^xsd:decimal .
<urn:validation:req> a veritas:ValidationCheckSpecification .
<urn:math:1> a veritas:MathematicalDiscoveryArtifact ;
  veritas:hasAxiomMap "A0,A1,A2,A3,A4,A5,A6,A7,A8,A9,A15" ;
  veritas:hasRepresentationMap <urn:repr:1> ;
  veritas:hasInvariant <urn:invariant:1> ;
  veritas:hasValidationRequirement <urn:validation:req> .
<urn:transfer:1> a veritas:TransferTestSpecification .
<urn:necessity:1> a veritas:GenerativeNecessityClaim ;
  veritas:supportedByEvidence <urn:evidence:1> ;
  veritas:hasProofStatus "speculative" ;
  veritas:testedByTransferTest <urn:transfer:1> .
<urn:test:1> a veritas:TestSpecification .
<urn:source:1> a veritas:SourceCodeArtifact ; veritas:validatedBy <urn:validation:req> ; veritas:testedBy <urn:test:1> .
<urn:build:1> a veritas:BuildArtifact ; veritas:hasStatus "production_candidate_validated" ; veritas:validatedBy <urn:validation:req> .
<urn:loop:1> a veritas:LoopSpecification ; veritas:hasTerminationCondition <urn:condition:1> .
"""


def incomplete_symbolic_shadow_ttl() -> str:
    return """@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .
<urn:formula:bad> a veritas:SymbolicShadow ; veritas:hasExpressionText "x+y" .
"""


def incomplete_math_artifact_ttl() -> str:
    return """@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .
<urn:math:bad> a veritas:MathematicalDiscoveryArtifact ; veritas:hasAxiomMap "A0" .
"""


def invalid_validated_build_ttl() -> str:
    return """@prefix veritas: <https://github.com/daddydrac/veritas/ontology#> .
<urn:build:bad> a veritas:BuildArtifact ; veritas:hasStatus "production_candidate_validated" .
"""


def source_mocked_phase5_summary(root: Path | None = None) -> dict[str, Any]:
    shapes = load_combined_shape_pack(root)
    shape_contract = shape_pack_contract(shapes)
    complete = validate_math_governance_contract(complete_math_to_code_ttl())
    bad_shadow = validate_math_governance_contract(incomplete_symbolic_shadow_ttl())
    bad_math = validate_math_governance_contract(incomplete_math_artifact_ttl())
    bad_build = validate_math_governance_contract(invalid_validated_build_ttl())
    findings_ttl = shacl_findings_to_turtle("phase5", bad_shadow["findings"] + bad_math["findings"] + bad_build["findings"])
    Graph().parse(data=findings_ttl, format="turtle")
    checks = [
        {"name": "combined_shape_pack_loads_core_and_math", "ok": shape_contract["ok"] and shape_contract["has_core_shapes"] and shape_contract["has_math_shapes"], "details": shape_contract},
        {"name": "complete_math_to_code_graph_conforms", "ok": complete["ok"], "details": complete},
        {"name": "missing_symbolic_shadow_obligations_blocked", "ok": not bad_shadow["ok"] and bad_shadow["finding_count"] >= 6, "details": bad_shadow},
        {"name": "missing_math_representation_obligations_blocked", "ok": not bad_math["ok"] and any(f["rule"] == "math.representation_map" for f in bad_math["findings"]), "details": bad_math},
        {"name": "validated_build_without_validation_blocked", "ok": not bad_build["ok"] and any(f["rule"] == "build.production_validation" for f in bad_build["findings"]), "details": bad_build},
        {"name": "shacl_findings_rdf_parseable", "ok": True, "details": {"bytes": len(findings_ttl)}},
    ]
    return {"ok": all(check["ok"] for check in checks), "checks": checks, "summary": {"checks": len(checks)}}
