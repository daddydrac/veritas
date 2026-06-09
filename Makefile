.PHONY: start up down ready ingest upload-ontology generate api-search api-sparql validate logs

start up:
	./scripts/bootstrap.sh

down:
	docker compose down

ready:
	curl -s http://localhost:8080/ready | jq

ingest:
	./scripts/ingest-demo.sh "cat:cs.AI OR cat:math.OC" 3

upload-ontology:
	./scripts/upload-ontology.sh

generate:
	./scripts/generate-code.sh "turn indexed research into tested Rust code" rust

api-search:
	curl -s http://localhost:8080/search -H 'content-type: application/json' -d '{"query":"invariant representation", "size":5}' | jq

api-sparql:
	curl -s http://localhost:8080/sparql -H 'content-type: application/json' -d '{"query":"PREFIX veritas: <https://github.com/daddydrac/veritas/ontology#> SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 25"}' | jq

validate:
	python scripts/validate-spec.py

logs:
	docker compose logs --tail=200
