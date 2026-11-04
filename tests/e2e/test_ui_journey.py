from __future__ import annotations

import pytest

SAMPLE = "[data-sample='film-shaky']"


def open_app(page, base, query="?lang=en"):
    page.goto(f"{base}/{query}", wait_until="networkidle")
    page.wait_for_selector("#panel .card")


def test_typed_question_through_to_a_spoken_result(live_app, page):
    open_app(page, live_app)
    assert page.locator("#pipeline .node").count() == 5
    assert page.locator(".mitthu").count() == 1

    page.click(SAMPLE)
    assert "చెత్త" in page.input_value("#ask-text")
    page.click("#ask-go")

    page.wait_for_selector("#plan-card")
    assert page.input_value("#plan-prompt").startswith("చెత్త")
    assert page.input_value("#plan-target") == "demo:mood-shaky"       # the sample picked the practice model
    page.click("[data-depth='quick']")
    page.click("#plan-start")

    page.wait_for_selector("#run-card")
    assert page.get_attribute(".mitthu", "data-mood") == "thinking"
    page.wait_for_function("document.querySelectorAll('.try').length >= 8")
    assert page.locator(".try mark").count() >= 2                       # changed words are highlighted

    page.wait_for_selector("#score-card", timeout=60_000)
    page.wait_for_selector(".read .s", timeout=30_000)
    page.wait_for_function("document.querySelectorAll('#pipeline .node.is-done').length === 5", timeout=30_000)
    assert 0 <= int(page.inner_text("#score-num")) <= 100
    body = page.inner_text("body")
    assert "<i>" not in body and "&lt;" not in body                      # nothing escaped leaks onto the page
    assert page.locator("#viz i").count() == 28

    page.click("#play")
    page.wait_for_function("document.querySelector('.mitthu').dataset.mood === 'speaking'", timeout=15_000)
    page.wait_for_selector("#read .s.is-now")
    page.click("#play")                                                  # pause

    page.click("#e-lang [data-l='hi']")
    page.wait_for_function("document.querySelector('#read') && document.querySelector('#read').lang === 'hi'", timeout=20_000)

    page.click("#toggle-tries")
    page.click("#tries .try >> nth=0")
    assert page.locator("#tries .try.is-open").count() == 1

    page.click("#btn-budget")
    page.wait_for_selector(".jar")
    assert page.locator(".jar").count() == 5
    page.keyboard.press("Escape")
    page.click("#btn-history")
    page.wait_for_selector(".hist [data-id]")
    page.keyboard.press("Escape")

    page.click(".langswitch [data-lang='te']")
    page.wait_for_function("document.querySelector('#score-card').innerText.includes('స్థిరత్వం')")
    assert page.problems == []


@pytest.mark.parametrize("seconds", [1.0, 4.0])
def test_voice_input_is_transcribed_into_the_box(live_app, page, seconds):
    open_app(page, live_app)
    page.click("#mic")
    page.wait_for_selector(".rec-layer")
    assert page.get_attribute(".mitthu", "data-mood") == "listening"
    page.wait_for_timeout(int(seconds * 1000))
    page.click("#mic")
    page.wait_for_function("document.querySelector('#ask-text').value.includes('అద్భుతం')", timeout=20_000)
    assert page.problems == []


@pytest.mark.parametrize("example", ["steady", "shaky"])
def test_bundled_examples_open_without_errors(live_app, page, example):
    open_app(page, live_app, f"?lang=en&demo={example}")
    page.wait_for_selector("#score-card")
    page.wait_for_function("document.querySelector('#score-num').textContent !== '0'")
    assert page.inner_text(".callout").startswith("Example result")
    grade = page.get_attribute(".verdict", "data-grade")
    assert grade == ("steady" if example == "steady" else "concerning")
    assert page.get_attribute(".mitthu", "data-mood") == ("happy" if example == "steady" else "worried")
    assert page.problems == []


def test_every_ui_language_renders_the_ask_view(live_app, page):
    for code, marker in (("te", "దేన్ని పరీక్షించాలి?"), ("hi", "किसे परखना है?"), ("en", "What should we check?")):
        open_app(page, live_app, f"?lang={code}")
        assert marker in page.inner_text("#ask-form")
    assert page.problems == []
