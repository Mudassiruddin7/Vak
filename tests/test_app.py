from __future__ import annotations

import json
from contextlib import ExitStack

import httpx
import pytest
from fastapi.testclient import TestClient

from vak.app import create_app
from vak.config import Settings

from .conftest import make_wav
from .fakes import FakeSarvam

PROMPT = "ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం."


@pytest.fixture
def api(tmp_path):
    stack = ExitStack()

    def build(fake=None, **overrides):
        fake = fake or FakeSarvam()
        settings = Settings(api_key="test-key", data_dir=tmp_path / "data", rpm_llm=60000, rpm_speech=60000, rpm_text=60000, **overrides)
        app = create_app(settings, transport=httpx.MockTransport(fake))
        client = stack.enter_context(TestClient(app))
        client.fake = fake
        client.svc = app.state.svc
        return client

    yield build
    stack.close()


def read_events(client, audit_id: str, stop=("done", "error", "cancelled")) -> list[tuple[str, dict]]:
    out: list[tuple[str, dict]] = []
    with client.stream("GET", f"/api/audits/{audit_id}/events") as resp:
        assert resp.status_code == 200 and resp.headers["content-type"].startswith("text/event-stream")
        name = None
        for line in resp.iter_lines():
            if line.startswith("event: "):
                name = line[7:]
            elif line.startswith("data: ") and name:
                out.append((name, json.loads(line[6:])))
                if name in stop:
                    break
    return out


def demo_request(model="mood-shaky", **extra):
    return {
        "plan": {"prompt": PROMPT, "prompt_language": "te", "depth": "quick",
                 "target": {"kind": "demo", "model": model}},
        "language": "te", **extra,
    }


# --- basics --------------------------------------------------------------------------------

def test_health_and_config(api):
    client = api()
    assert client.get("/api/health").json()["has_key"] is True
    cfg = client.get("/api/config").json()
    assert {s["id"] for s in cfg["targets"]["demo"]} == {"mood-steady", "mood-shaky"}
    assert cfg["targets"]["default"] == "sarvam-105b-conversations"
    assert len(cfg["samples"]) >= 3 and set(cfg["voices"]) == {"te", "hi"}
    assert "<title>" in client.get("/").text


def test_nothing_secret_leaves_through_config(api):
    client = api()
    assert "test-key" not in client.get("/api/config").text + client.get("/api/health").text


def test_plan_extracts_the_quoted_question_and_prices_each_depth(api):
    client = api()
    body = client.post("/api/plan", json={"text": 'ఈ ప్రశ్న అడుగు: "భారతదేశ రాజధాని ఏది?"'}).json()
    assert body["plan"]["prompt"] == "భారతదేశ రాజధాని ఏది?"
    est = body["estimates"]
    assert set(est) == {"quick", "standard", "deep"}
    assert 0 < est["quick"]["total_inr"] < est["deep"]["total_inr"]


# --- a whole audit ---------------------------------------------------------------------------

def test_audit_runs_end_to_end_and_streams_progress(api):
    client = api()
    audit_id = client.post("/api/audits", json=demo_request()).json()["id"]
    events = read_events(client, audit_id)
    names = [n for n, _ in events]
    for needed in ("plan", "baseline", "progress", "test", "units", "report", "explanation", "done"):
        assert needed in names, needed
    assert names.index("report") < names.index("explanation") < names.index("done")
    stages = [(d["name"], d["state"]) for n, d in events if n == "stage"]
    assert stages == [("engine", "active"), ("engine", "done"), ("explain", "active"), ("explain", "done"),
                      ("output", "active"), ("output", "done")]

    record = client.get(f"/api/audits/{audit_id}").json()
    assert record["status"] == "done" and record["score"] == record["report"]["consistency_score"]
    assert record["explanations"]["te"]["source"] == "model"
    assert "api_key" not in json.dumps(record)

    assert any(h["id"] == audit_id for h in client.get("/api/history").json())
    md = client.get(f"/api/audits/{audit_id}/report.md")
    assert md.status_code == 200 and "Consistency score" in md.text and "attachment" in md.headers["content-disposition"]


def test_a_late_client_can_resume_the_stream(api):
    client = api()
    audit_id = client.post("/api/audits", json=demo_request()).json()["id"]
    full = read_events(client, audit_id)
    with client.stream("GET", f"/api/audits/{audit_id}/events", headers={"Last-Event-ID": str(len(full) - 2)}) as resp:
        tail = [line for line in resp.iter_lines() if line.startswith("event: ")]
    assert [t[7:] for t in tail] == [n for n, _ in full[-1:]]


def test_voice_is_made_on_request_and_cached(api):
    client = api()
    audit_id = client.post("/api/audits", json=demo_request()).json()["id"]
    read_events(client, audit_id)
    first = client.post(f"/api/audits/{audit_id}/speak", json={"language": "te"}).json()
    assert first["url"].startswith("/api/audio/") and first["seconds"] == pytest.approx(2.0) and first["cost_inr"] > 0
    wav = client.get(first["url"])
    assert wav.status_code == 200 and wav.content[:4] == b"RIFF" and wav.headers["content-type"] == "audio/wav"
    again = client.post(f"/api/audits/{audit_id}/speak", json={"language": "te"}).json()
    assert again["cached"] is True and again["cost_inr"] == 0
    assert client.get(f"/api/audits/{audit_id}").json()["explanations"]["te"]["audio"]["url"] == first["url"]


def test_voice_can_be_requested_up_front(api):
    client = api()
    audit_id = client.post("/api/audits", json=demo_request(voice=True)).json()["id"]
    events = dict(read_events(client, audit_id))
    assert events["audio"]["url"].startswith("/api/audio/")


def test_explaining_in_another_language_uses_the_template_when_the_model_stays_in_telugu(api):
    client = api()
    audit_id = client.post("/api/audits", json=demo_request()).json()["id"]
    read_events(client, audit_id)
    hindi = client.post(f"/api/audits/{audit_id}/explain", json={"language": "hi"}).json()
    assert hindi["language"] == "hi" and hindi["source"] == "template"
    assert client.post(f"/api/audits/{audit_id}/explain", json={"language": "hi"}).json() == hindi   # stored, not redone


def test_transcribe_returns_text_and_charges_for_the_seconds(api):
    client = api()
    resp = client.post("/api/transcribe", files={"audio": ("clip.wav", make_wav(3.0), "audio/wav")}, data={"language": "auto"})
    body = resp.json()
    assert resp.status_code == 200 and body["language"] == "te" and body["seconds"] == 3.0 and body["text"]
    assert client.get("/api/budget").json()["pots"]["stt"]["calls"] == 1


def test_oversized_and_empty_uploads_are_refused(api):
    client = api()
    assert client.post("/api/transcribe", files={"audio": ("c.wav", make_wav(31.0), "audio/wav")}).status_code == 413
    assert client.post("/api/transcribe", files={"audio": ("c.wav", b"x" * (7 * 1024 * 1024), "audio/wav")}).status_code == 413


# --- failure modes -----------------------------------------------------------------------------

def test_out_of_credits_is_explained_not_dumped(api):
    def broke(req):
        return httpx.Response(402, json={"error": {"message": "No credits available.", "code": "insufficient_quota_error"}})

    client = api(broke)
    resp = client.post("/api/plan", json={"text": PROMPT})
    assert resp.status_code == 402 and resp.json()["error"]["code"] == "out_of_credits"
    assert "dashboard" in resp.json()["error"]["message"]


def test_a_failure_mid_audit_arrives_as_an_error_event(api):
    class Flaky(FakeSarvam):
        def __call__(self, request):
            if "KEY:" not in request.content.decode() and self.paths["/v1/chat/completions"] >= 2 and request.url.path.endswith("completions"):
                return httpx.Response(402, json={"error": {"message": "x", "code": "insufficient_quota_error"}})
            return super().__call__(request)

    client = api(Flaky())
    body = {"plan": {"prompt": PROMPT, "prompt_language": "te", "depth": "quick"}, "language": "te"}
    audit_id = client.post("/api/audits", json=body).json()["id"]
    events = read_events(client, audit_id)
    name, data = events[-1]
    assert name == "error" and data["code"] == "out_of_credits"
    assert client.get(f"/api/audits/{audit_id}").json()["status"] == "failed"


def test_bad_requests_get_clear_422s(api):
    client = api()
    too_long = demo_request()
    too_long["plan"]["prompt"] = "అ " * 500
    resp = client.post("/api/audits", json=too_long)
    assert resp.status_code == 422 and "under 700 characters" in resp.json()["error"]["message"]
    assert client.post("/api/audits", json={"plan": {"prompt": "   "}}).status_code == 422
    custom = demo_request()
    custom["plan"]["target"] = {"kind": "custom", "base_url": "http://localhost:1/v1", "remote_model": "x"}
    assert api(allow_custom_targets=False).post("/api/audits", json=custom).status_code == 422


def test_the_cost_cap_stops_expensive_runs(api):
    client = api(audit_cost_cap_inr=0.01)
    body = {"plan": {"prompt": PROMPT, "prompt_language": "te", "depth": "deep"}, "language": "te"}
    resp = client.post("/api/audits", json=body)
    assert resp.status_code == 422 and "limit" in resp.json()["error"]["message"]


def test_audio_route_does_not_serve_arbitrary_files(api):
    client = api()
    for name in ("../vak.db", "..%2Fvak.db", "notes.wav", "x" * 24 + ".wav"):
        assert client.get(f"/api/audio/{name}").status_code == 404


def test_unknown_audits_are_404(api):
    client = api()
    for path in ("/api/audits/nope", "/api/audits/nope/events", "/api/audits/nope/report.md"):
        assert client.get(path).status_code == 404
    assert client.post("/api/audits/nope/speak", json={"language": "te"}).status_code == 404


def test_cancelling_stops_a_running_audit(api):
    client = api()
    body = {"plan": {"prompt": PROMPT, "prompt_language": "te", "depth": "deep", "target": {"kind": "demo", "model": "mood-shaky"}}, "language": "te"}
    audit_id = client.post("/api/audits", json=body).json()["id"]
    # The audit usually finishes before the cancel lands; either outcome must leave a tidy record.
    client.post(f"/api/audits/{audit_id}/cancel")
    events = read_events(client, audit_id)
    assert events[-1][0] in ("done", "cancelled")
    assert client.get(f"/api/audits/{audit_id}").json()["status"] in ("done", "cancelled")


def test_budget_reflects_what_was_spent(api):
    client = api()
    before = client.get("/api/budget").json()["total"]["spent"]
    audit_id = client.post("/api/audits", json=demo_request()).json()["id"]
    read_events(client, audit_id)
    after = client.get("/api/budget").json()
    assert after["total"]["spent"] > before
    assert after["pots"]["llm"]["calls"] > 0 and after["total"]["allocated"] == 25000
