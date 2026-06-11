from __future__ import annotations
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from pyshacl import validate
from rdflib import Graph

app = FastAPI(title="Veritas SHACL Validator")

class ValidateRequest(BaseModel):
    data_ttl: str
    shapes_ttl: str | None = None

@app.get('/health')
def health():
    return {"ok": True, "service": "veritas-shacl"}

@app.post('/validate')
def validate_ttl(req: ValidateRequest):
    data_graph = Graph().parse(data=req.data_ttl, format='turtle')
    shapes_graph = Graph().parse(data=req.shapes_ttl, format='turtle') if req.shapes_ttl else None
    conforms, results_graph, results_text = validate(data_graph, shacl_graph=shapes_graph, inference='rdfs', abort_on_first=False, allow_infos=True, allow_warnings=True)
    return {"ok": True, "conforms": bool(conforms), "results_text": results_text, "results_ttl": results_graph.serialize(format='turtle')}
