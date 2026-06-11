#!/usr/bin/env bash
set -euo pipefail
API_URL=${VERITAS_API_URL:-http://localhost:${VERITAS_API_PORT:-8080}}
python3 - <<'PY'
import json, urllib.request, os, pathlib
api=os.environ.get('VERITAS_API_URL', f"http://localhost:{os.environ.get('VERITAS_API_PORT','8080')}")
path=pathlib.Path('packages/ontology/veritas.owl')
body=json.dumps({"graph_uri":"urn:veritas:graph:ontology","turtle":path.read_text(),"replace":True,"content_type":"application/rdf+xml"}).encode()
req=urllib.request.Request(api + '/graph/upload', data=body, headers={'content-type':'application/json'}, method='POST')
with urllib.request.urlopen(req, timeout=60) as resp:
    payload=resp.read().decode()
    pathlib.Path('data/e2e').mkdir(parents=True, exist_ok=True)
    pathlib.Path('data/e2e/ontology-upload.json').write_text(payload)
    print(payload)
PY
