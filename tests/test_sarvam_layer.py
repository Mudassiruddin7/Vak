from __future__ import annotations

import json

import httpx
import pytest

from vak.audio_util import clean_for_tts, merge_wavs, split_for_tts, wav_seconds
from vak.budget import Budget, BudgetExhausted, estimate_audit
from vak.sarvam import AuthError, BadRequest, OutOfCredits, Unavailable
from vak.store import Store
from vak.util import extract_json, strip_think

from .conftest import chat_reply, make_wav, wav_b64


def err(status: int, code: str, message: str = "nope", headers: dict | None = None) -> httpx.Response:
    return httpx.Response(status, json={"error": {"message": message, "code": code}}, headers=headers or {})


# --- http layer -----------------------------------------------------------

async def test_retries_a_429_then_succeeds(wire_factory):
    answers = iter([err(429, "rate_limit_exceeded_error"), httpx.Response(200, json={"ok": True})])
    wire = wire_factory(lambda req: next(answers))
    resp = await wire.http.request("llm", "POST", "/v1/chat/completions", json={})
    assert resp.json() == {"ok": True}
    assert wire.calls == 2


async def test_402_is_not_retried(wire_factory):
    wire = wire_factory(lambda req: err(402, "insufficient_quota_error", "No credits available."))
    with pytest.raises(OutOfCredits):
        await wire.http.request("llm", "POST", "/v1/chat/completions", json={})
    assert wire.calls == 1


async def test_seat_budget_429_is_not_retried(wire_factory):
    wire = wire_factory(lambda req: err(429, "seat_budget_exhausted_error"))
    with pytest.raises(Exception) as caught:
        await wire.http.request("llm", "POST", "/x", json={})
    assert caught.value.code == "seat_budget_exhausted_error"
    assert wire.calls == 1


async def test_403_means_bad_key(wire_factory):
    wire = wire_factory(lambda req: err(403, "invalid_api_key_error"))
    with pytest.raises(AuthError):
        await wire.http.request("llm", "POST", "/x", json={})
    assert wire.calls == 1


async def test_gives_up_after_repeated_503(wire_factory):
    wire = wire_factory(lambda req: err(503, "model_overloaded"))
    with pytest.raises(Unavailable):
        await wire.http.request("llm", "POST", "/x", json={})
    assert wire.calls == 4


async def test_network_errors_become_unavailable(wire_factory):
    def boom(req):
        raise httpx.ConnectError("no route")

    wire = wire_factory(boom)
    with pytest.raises(Unavailable) as caught:
        await wire.http.request("llm", "POST", "/x", json={})
    assert "ConnectError" in str(caught.value)
    assert wire.calls == 4


async def test_missing_key_fails_before_any_request(wire_factory, settings):
    from dataclasses import replace
    from vak.sarvam import SarvamHTTP

    http = SarvamHTTP(replace(settings, api_key=""))
    with pytest.raises(AuthError):
        await http.request("llm", "POST", "/x", json={})
    await http.aclose()


async def test_key_goes_in_the_header_only(wire_factory):
    wire = wire_factory(lambda req: httpx.Response(200, json={}))
    await wire.http.request("llm", "POST", "/x", json={"a": 1})
    req = wire.requests[0]
    assert req.headers["api-subscription-key"] == "test-key"
    assert b"test-key" not in req.content


# --- chat -----------------------------------------------------------------

async def test_chat_caches_deterministic_calls_and_meters_once(clients, budget):
    wire = clients(lambda req: chat_reply("ఇదిగో జవాబు", prompt=1000, completion=500))
    msgs = [{"role": "user", "content": "hi"}]
    first = await wire.chat.complete("sarvam-105b-conversations", msgs)
    second = await wire.chat.complete("sarvam-105b-conversations", msgs)
    assert first.text == second.text == "ఇదిగో జవాబు"
    assert not first.cached and second.cached
    assert wire.calls == 1
    llm = budget.summary()["pots"]["llm"]
    assert llm["calls"] == 1
    assert llm["spent"] == pytest.approx((1000 * 15 + 500 * 60) / 1_000_000)


async def test_chat_does_not_cache_sampled_calls(clients):
    wire = clients(lambda req: chat_reply("x"))
    msgs = [{"role": "user", "content": "hi"}]
    await wire.chat.complete("sarvam-105b", msgs, temperature=0.8)
    await wire.chat.complete("sarvam-105b", msgs, temperature=0.8)
    assert wire.calls == 2


async def test_chat_routes_models_and_switches_thinking_off(clients):
    wire = clients(lambda req: chat_reply("x"))
    msgs = [{"role": "user", "content": "hi"}]
    await wire.chat.complete("sarvam-105b", msgs)
    await wire.chat.complete("gemma4", msgs)
    await wire.chat.complete("deepseekv4.1-flash-1017", msgs)
    first, second, third = (json.loads(r.content) for r in wire.requests)
    assert wire.requests[0].url.path == "/v1/chat/completions"
    assert "reasoning_effort" in first and first["reasoning_effort"] is None
    assert wire.requests[1].url.path == "/v2/chat/completions"
    assert "reasoning_effort" not in second
    assert third["reasoning_effort"] == "none"


async def test_chat_strips_think_blocks_and_handles_n(clients):
    wire = clients(lambda req: chat_reply("<think>hmm</think>answer", n=3))
    out = await wire.chat.complete("sarvam-105b", [{"role": "user", "content": "q"}], temperature=0.7, n=3)
    assert out.texts == ["answer"] * 3
    assert json.loads(wire.requests[0].content)["n"] == 3


async def test_chat_caps_max_tokens(clients):
    wire = clients(lambda req: chat_reply("x"))
    await wire.chat.complete("sarvam-105b", [{"role": "user", "content": "q"}], max_tokens=99999)
    assert json.loads(wire.requests[0].content)["max_tokens"] == 4096


# --- speech and text ------------------------------------------------------

async def test_transcribe_meters_by_the_second_and_sends_multipart(clients, budget):
    wire = clients(lambda req: httpx.Response(200, json={"transcript": " నమస్తే ", "language_code": "te-IN"}))
    out = await wire.speech.transcribe(make_wav(6.0), language="auto")
    assert out.text == "నమస్తే" and out.language == "te"
    body = wire.requests[0].content
    assert b'name="model"' in body and b"saaras:v4" in body
    assert b"unknown" in body
    assert budget.summary()["pots"]["stt"]["spent"] == pytest.approx(6 / 60 * 0.49, rel=1e-3)


async def test_transcribe_refuses_long_or_empty_clips(clients):
    wire = clients(lambda req: httpx.Response(200, json={"transcript": "x"}))
    with pytest.raises(BadRequest):
        await wire.speech.transcribe(make_wav(31.0))
    with pytest.raises(BadRequest):
        await wire.speech.transcribe(make_wav(0.1))
    assert wire.calls == 0


async def test_synthesize_writes_a_cached_wav(clients, budget):
    wire = clients(lambda req: httpx.Response(200, json={"audios": [wav_b64(2.0)]}))
    first = await wire.speech.synthesize("నమస్కారం. ఇది ఒక పరీక్ష.", "te")
    again = await wire.speech.synthesize("నమస్కారం. ఇది ఒక పరీక్ష.", "te")
    assert first.path.exists() and first.seconds == pytest.approx(2.0)
    assert again.cached and wire.calls == 1
    body = json.loads(wire.requests[0].content)
    assert body["language_code"] == "te-IN" and body["speaker"] == "kavitha_te_conversation"
    chars = len("నమస్కారం. ఇది ఒక పరీక్ష.")
    assert budget.summary()["pots"]["tts"]["spent"] == pytest.approx(chars / 1000 * 1.0)


async def test_synthesize_falls_back_to_default_voice(clients):
    calls = []

    def handler(req):
        body = json.loads(req.content)
        calls.append(body["speaker"])
        if body["speaker"] == "ghost":
            return err(400, "invalid_request_error", "speaker not valid")
        return httpx.Response(200, json={"audios": [wav_b64(1.0)]})

    wire = clients(handler)
    out = await wire.speech.synthesize("హలో", "te", speaker="ghost")
    assert calls == ["ghost", "kavitha_te_conversation"]
    assert out.path.exists()


async def test_translate_and_transliterate_cache_and_meter(clients, budget):
    def handler(req):
        if req.url.path == "/translate":
            return httpx.Response(200, json={"translated_text": "नमस्ते"})
        return httpx.Response(200, json={"transliterated_text": "namaste"})

    wire = clients(handler)
    assert await wire.text.translate("నమస్తే", "te", "hi") == "नमस्ते"
    assert await wire.text.translate("నమస్తే", "te", "hi") == "नमस्ते"
    assert await wire.text.transliterate("నమస్తే", "te", "en") == "namaste"
    assert wire.calls == 2
    assert budget.summary()["pots"]["translate"]["calls"] == 2
    assert await wire.text.translate("same", "en", "en") == "same"


# --- helpers --------------------------------------------------------------

def test_extract_json_handles_fences_and_noise():
    assert extract_json('{"a": 1}') == {"a": 1}
    assert extract_json('Sure!\n```json\n{"a": [1, 2]}\n```') == {"a": [1, 2]}
    assert extract_json('prefix {"a": "}"} suffix') == {"a": "}"}
    assert extract_json("nothing here") is None


def test_strip_think_removes_closed_and_unclosed_blocks():
    assert strip_think("<think>a</think>hello") == "hello"
    assert strip_think("hello<think>cut off mid-thought") == "hello"


def test_split_for_tts_respects_the_limit_and_keeps_order():
    text = " ".join(f"వాక్యం సంఖ్య {i}." for i in range(80))
    parts = split_for_tts(text, 200)
    assert all(len(p) <= 200 for p in parts) and len(parts) > 3
    assert " ".join(parts).replace("  ", " ") == text


def test_clean_for_tts_adds_commas_and_drops_markup():
    assert clean_for_tts("**ఖర్చు** 125000 రూపాయలు") == "ఖర్చు 125,000 రూపాయలు"


def test_merge_wavs_adds_up():
    merged = merge_wavs([make_wav(1.0), make_wav(2.0)], gap_ms=200)
    assert wav_seconds(merged) == pytest.approx(3.2, abs=0.01)


# --- budget ---------------------------------------------------------------

def test_overspend_draws_from_the_buffer():
    store = Store(":memory:")
    budget = Budget(store, {"stt": 1, "tts": 1, "llm": 1, "translate": 1, "buffer": 5})
    budget.record("llm", 10, "tokens", 3.0)
    s = budget.summary()
    assert s["pots"]["llm"]["remaining"] == 0
    assert s["buffer"]["spent"] == 2.0 and s["buffer"]["remaining"] == 3.0
    budget.ensure({"llm": 2.5})
    with pytest.raises(BudgetExhausted):
        budget.ensure({"llm": 3.5})


def test_estimates_grow_with_depth():
    base = dict(prompt_chars=120, target_model="sarvam-105b-conversations", helper_model="sarvam-105b")
    quick = estimate_audit("quick", **base)
    deep = estimate_audit("deep", **base)
    assert 0 < quick.total_inr < deep.total_inr < 15
    assert estimate_audit("quick", voice=True, **base).tts_inr > 0
