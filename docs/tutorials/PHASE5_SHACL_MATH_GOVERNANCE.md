# Phase 5 — SHACL and Mathematical Governance

Phase 5 closes the source/mocked governance gap for Veritas. It makes the automatic SHACL gate load both the core engineering rule pack and the representation-first math rule pack, then validates the merged plan/run/document context before code generation can proceed.

Live Docker, Cargo, and vLLM validation remain host-validation-pending in this scoped pass. The source/mocked proof demonstrates that the rule packs, RDF contracts, and governance behavior are internally consistent without requiring a live SHACL container.

## What Phase 5 enforces

The automatic gate now combines:

```text
packages/ontology/shacl/veritas-core.shacl.ttl
packages/ontology/shacl/veritas-math.shacl.ttl
```

The core rules enforce production engineering obligations:

```text
ResearchObjective -> Plan
Plan -> TaskSpecification + ValidationCheckSpecification
Risk -> mitigation / acceptance / blocked status
SourceCodeArtifact -> validation + tests
BuildArtifact -> explicit status + validation when production_candidate_validated
LoopSpecification -> termination condition
Finding -> derivedFrom + status + description
```

The math rules enforce representation-first research-to-code obligations:

```text
SymbolicShadow -> evidence + expression + normalized expression + formula source
SymbolicShadow -> formula image status + LaTeX OCR status + confidence + human validation status
RepresentationMap -> surface phenomenon + latent structure + preserved invariant
Invariant -> transformation family
GenerativeNecessityClaim -> evidence + proof status + transfer test
MathematicalDiscoveryArtifact -> axiom map + representation map + invariant/status + validation requirement
```

This keeps formulas in their correct role: they are symbolic shadows of deeper transformational structure, not truth by themselves. A formula cannot become production code unless its representation, invariant, validation, and transfer/proof obligations are explicit.

## Source/mocked proof

Run:

```bash
scripts/e2e/source-mocked-shacl-governance.sh
```

The proof checks:

```text
combined_shape_pack_loads_core_and_math
complete_math_to_code_graph_conforms
missing_symbolic_shadow_obligations_blocked
missing_math_representation_obligations_blocked
validated_build_without_validation_blocked
shacl_findings_rdf_parseable
```

The summary is written to:

```text
data/e2e/source-mocked-shacl-governance/phase5-summary.json
```

## Host validation boundary

This phase does not claim live pySHACL/Docker execution. Live validation still requires:

```bash
scripts/validate-host.sh --profile host-prod
```

or a GPU profile when live vLLM is required:

```bash
scripts/production-acceptance.sh --profile single-gpu-prod
```
