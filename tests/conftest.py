from __future__ import annotations

import base64
import io
import wave
from pathlib import Path

import httpx
import pytest

from vak.budget import Budget
from vak.config import Settings
from vak.sarvam import ChatClient, SarvamHTTP, SpeechClient, TextClient
from vak.store import Store


def make_wav(seconds: float = 1.0, rate: int = 16000) -> bytes:
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(b"\x00\x00" * int(rate * seconds))
    return buf.getvalue()


def wav_b64(seconds: float = 1.0) -> str:
    return base64.b64encode(make_wav(seconds)).decode()


@pytest.fixture
def settings(tmp_path: Path) -> Settings:
    return Settings(api_key="test-key", data_dir=tmp_path / "data", rpm_llm=6000, rpm_speech=6000, rpm_text=6000)


@pytest.fixture
def store(settings: Settings) -> Store:
    s = Store(":memory:")
    yield s
    s.close()


@pytest.fixture
def budget(store: Store, settings: Settings) -> Budget:
    return Budget(store, settings.budget)


class Wire:
    """A fake api.sarvam.ai. Give it a handler, it records every request."""

    def __init__(self, handler):
        self.handler = handler
        self.requests: list[httpx.Request] = []
        self.transport = httpx.MockTransport(self._dispatch)

    def _dispatch(self, request: httpx.Request) -> httpx.Response:
        self.requests.append(request)
        return self.handler(request)

    @property
    def calls(self) -> int:
        return len(self.requests)


@pytest.fixture
def wire_factory(settings: Settings):
    made: list[SarvamHTTP] = []

    def build(handler):
        wire = Wire(handler)
        http = SarvamHTTP(settings, transport=wire.transport)
        http._backoff = lambda attempt: 0.0   # no real waiting in tests
        made.append(http)
        wire.http = http
        return wire

    yield build


def chat_reply(text: str = "ok", *, prompt: int = 10, completion: int = 5, n: int = 1) -> httpx.Response:
    choices = [{"index": i, "finish_reason": "stop", "message": {"role": "assistant", "content": text}} for i in range(n)]
    return httpx.Response(200, json={"choices": choices, "usage": {"prompt_tokens": prompt, "completion_tokens": completion}})


@pytest.fixture
def clients(wire_factory, settings, store, budget):
    def build(handler):
        wire = wire_factory(handler)
        wire.chat = ChatClient(wire.http, store, budget)
        wire.speech = SpeechClient(wire.http, settings, budget)
        wire.text = TextClient(wire.http, settings, store, budget)
        return wire

    return build
