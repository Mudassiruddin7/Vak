# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Walks the real UI in a real browser against a running server. It never calls a paid API:
it uses the recorded example audits, the writing-rule checker and the Setu screen's layout.

    cargo run -p vak-server &            # or any deployed URL
    uv run tests/e2e/journey.py http://127.0.0.1:8420
"""
import sys
from playwright.sync_api import sync_playwright, expect

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8420"

with sync_playwright() as p:
    b = p.firefox.launch()
    page = b.new_page(viewport={"width": 1280, "height": 900})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)

    page.goto(BASE)
    expect(page.get_by_role("heading", level=1)).to_contain_text("ask again")

    # a finished audit, in English then Hindi
    page.get_by_text("See a finished audit").click()
    expect(page.locator(".verdict")).to_be_visible()
    expect(page.locator("#pct")).not_to_have_text("0", timeout=4000)
    page.locator(".langpick [data-lang=hi]").click()
    expect(page.get_by_text("मॉडल की जाँच")).to_be_visible()

    page.locator(".langpick [data-lang=en]").click()

    # Translate: the tab opens with two panes and a language picker
    page.get_by_role("link", name="Translate", exact=True).first.click()
    expect(page.locator("#in")).to_be_visible()
    expect(page.locator("[data-all=dst]")).to_be_visible()

    # Setu: both people's controls are there and nothing is spoken without consent in a sensitive setting
    page.get_by_role("link", name="Setu", exact=True).first.click()
    expect(page.locator("[data-mic]")).to_have_count(2)
    page.select_option("#dm", "public_service")
    expect(page.locator("#agree")).to_be_visible()
    expect(page.locator("[data-mic=a]")).to_be_disabled()
    page.locator("#agree").click()
    expect(page.locator("[data-mic=a]")).to_be_enabled()

    # Learn: a lesson question can be answered and gives feedback
    page.get_by_role("link", name="Learn", exact=True).first.click()
    page.locator("[data-lesson='0']").click()
    page.get_by_role("button", name="Yes, it changed").click()
    expect(page.locator("#sheet [role=status]")).to_be_visible()

    # Rules: the checker flags a sentence that is too long
    page.locator("a", has_text="Rules").first.click()
    page.locator("#txt").fill(" ".join(["పదం"] * 20) + ".")
    page.get_by_role("button", name="Check it").click()
    expect(page.locator("#out .err-box")).to_contain_text("Sentences over the limit")

    assert not errors, errors
    print("ok")
    b.close()
