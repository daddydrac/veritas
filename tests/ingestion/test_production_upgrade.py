from __future__ import annotations

import json
from pathlib import Path

from rdflib import Graph

from veritas_ingest.chunking import make_chunks
from veritas_ingest.citations import build_apa_citation, citation_from_metadata
from veritas_ingest.formulas import extract_formulas
from veritas_ingest.sinks import chunks_to_turtle, ensure_index


class _FakeIndices:
    def __init__(self):
        self.created = None
    def exists(self, index: str) -> bool:
        return False
    def create(self, index: str, body: dict) -> None:
        self.created = (index, body)

class _FakeClient:
    def __init__(self):
        self.indices = _FakeIndices()


def test_25_word_chunking_extends_to_sentence_boundary_and_preserves_formula() -> None:
    text = (
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen "
        "sixteen seventeen eighteen nineteen twenty twentyone twentytwo twentythree twentyfour twentyfive "
        "twentysix; second sentence keeps $$E=mc^2$$ intact and searchable."
    )
    chunks = make_chunks("paper", text, {"title": "Fixture", "chunk_target_words": 25}, 25, 0, 400, 50)
    assert chunks[0]["text"].endswith("twentysix;")
    assert any(chunk["chunk_type"] == "formula" for chunk in chunks)
    assert sum(len(chunk.get("formulas", [])) for chunk in chunks) >= 2


def test_apa_citation_builder_and_metadata_record() -> None:
    citation = build_apa_citation("A Test of Veritas", ["Jane Doe", "Alan Smith"], "2026-06-01", source_url="https://arxiv.org/abs/2601.00001")
    assert citation.startswith("Doe, J., & Smith, A. (2026).")
    meta = citation_from_metadata({"title": "A Test", "authors": ["Jane Doe"], "published": "2026-01-01", "entry_url": "https://example.test"})
    assert meta["apa_citation"].startswith("Doe, J. (2026). A Test.")
    assert meta["status"] == "machine_generated_pending_human_review"


def test_opensearch_mapping_contains_formula_and_citation_fields() -> None:
    client = _FakeClient()
    ensure_index(client, "veritas-test", {"field": "embedding", "dimension": 768})
    mapping = client.indices.created[1]["mappings"]["properties"]
    assert mapping["chunk_id"]["type"] == "keyword"
    assert mapping["text"]["type"] == "text"
    assert mapping["embedding"]["type"] == "knn_vector"
    assert mapping["embedding"]["method"]["engine"] == "faiss"
    assert mapping["formulas"]["type"] == "nested"
    assert mapping["formulas"]["properties"]["normalized_latex"]["fields"]["raw"]["type"] == "keyword"
    assert mapping["apa_citation"]["fields"]["keyword"]["type"] == "keyword"


def test_turtle_contains_citation_and_symbolic_shadow() -> None:
    text = "Energy relation $$E=mc^2$$."
    formulas = extract_formulas(text)
    chunks = [{
        "chunk_id": "paper::chunk::00000",
        "paper_id": "paper",
        "ordinal": 0,
        "text": text,
        "formulas": formulas,
        "metadata": {"title": "Relativity", "apa_citation": "Einstein, A. (1905). Relativity.", "authors": ["Albert Einstein"], "year": "1905"},
    }]
    ttl = chunks_to_turtle(chunks, "https://github.com/daddydrac/veritas/ontology#", "urn:test")
    assert "bibliographicCitation" in ttl
    assert "SymbolicShadow" in ttl
    Graph().parse(data=ttl, format="turtle")


def test_shacl_rule_pack_exists_and_targets_veritas_classes() -> None:
    ttl = Path("packages/ontology/shacl/veritas-core.shacl.ttl").read_text()
    assert "veritas:SymbolicShadowShape" in ttl
    assert "veritas:GenerativeNecessityClaimShape" in ttl
    assert "veritas:BuildArtifactShape" in ttl
