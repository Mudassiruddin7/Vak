"""Browser tests run the real app in a thread, against the offline fake Sarvam backend.

They need Playwright and a browser (`pip install -e ".[e2e]" && playwright install firefox`)
and skip themselves quietly when either is missing.
"""
from __future__ import annotations

import asyncio
import socket
import threading
import time

import httpx
import pytest

pytest.importorskip("playwright.sync_api")
import uvicorn  # noqa: E402
from playwright.sync_api import Error as PlaywrightError  # noqa: E402
from playwright.sync_api import sync_playwright  # noqa: E402

from vak.app import create_app  # noqa: E402
from vak.config import Settings  # noqa: E402

from ..fakes import FakeSarvam  # noqa: E402


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture
def live_app(tmp_path):
    """The app on a real port, with a little latency so the run view can be watched."""
    fake = FakeSarvam()

    async def with_latency(request: httpx.Request) -> httpx.Response:
        await asyncio.sleep(0.25)
        return fake(request)

    port = _free_port()
    settings = Settings(api_key="test-key", data_dir=tmp_path / "data", port=port,
                        rpm_llm=60000, rpm_speech=60000, rpm_text=60000)
    app = create_app(settings, transport=httpx.MockTransport(with_latency))
    server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=port, log_level="error"))
    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{port}"
    for _ in range(100):
        try:
            if httpx.get(f"{base}/api/health", timeout=1).status_code == 200:
                break
        except httpx.HTTPError:
            time.sleep(0.1)
    else:
        pytest.fail("the test server never came up")
    yield base
    server.should_exit = True
    thread.join(timeout=5)


@pytest.fixture
def page(request):
    """A fresh page with a fake microphone. Console errors are collected on `page.problems`."""
    with sync_playwright() as p:
        browser = None
        for name, launch in (
            ("firefox", dict(firefox_user_prefs={"media.navigator.streams.fake": True, "media.navigator.permission.disabled": True})),
            ("chromium", dict(args=["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream"])),
        ):
            try:
                browser = getattr(p, name).launch(**launch)
                break
            except PlaywrightError:
                continue
        if browser is None:
            pytest.skip("no Playwright browser installed (run: playwright install firefox)")
        context = browser.new_context(viewport={"width": 1280, "height": 900}, locale="en-US")
        pg = context.new_page()
        pg.problems = []
        pg.on("pageerror", lambda e: pg.problems.append(f"page error: {e}"))
        pg.on("console", lambda m: pg.problems.append(f"console: {m.text}") if m.type == "error" else None)
        yield pg
        browser.close()
