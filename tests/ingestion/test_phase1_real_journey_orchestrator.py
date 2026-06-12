from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def test_api_exposes_real_journey_routes_and_module():
    main = read("apps/api/src/main.rs")
    journey = read("apps/api/src/journey.rs")
    assert "mod journey;" in main
    for route in [
        '/journey/run',
        '/journey/:run_id/status',
        '/journey/:run_id/review',
        '/journey/:run_id/resume',
        '/journey/:run_id/report',
    ]:
        assert route in main
    assert "JourneyRunRequest" in journey
    assert "execute_autonomous_run_core" in journey
    assert "real_product_path" in journey
    assert "mocked\": false" in journey


def test_journey_persists_real_lifecycle_artifacts():
    journey = read("apps/api/src/journey.rs")
    for artifact in [
        "journey_request.json",
        "source_manifest.json",
        "request.json",
        "journey_state.json",
        "journey_lifecycle.jsonl",
        "journey_report.json",
        "final_report.json",
        "human_checkpoints.jsonl",
    ]:
        assert artifact in journey
    assert "SourceRegistered" in journey
    assert "JourneyCompleted" in journey
    assert "HumanCheckpointRecorded" in journey


def test_cli_has_single_real_journey_entrypoint():
    cli = read("apps/cli/src/main.rs")
    assert "enum JourneyCommands" in cli
    assert "JourneyCommands::Run" in cli
    assert "api.journey.run" in cli
    assert "api.journey.status" in cli
    assert "api.journey.review" in cli
    assert "api.journey.resume" in cli
    assert "api.journey.report" in cli
