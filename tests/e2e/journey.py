# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""Walks the real UI in a real browser against a running server. It never calls a paid API:
it uses the recorded example audits, the writing-rule checker and the screens' layout.

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

    def nav(name):
        page.locator(f'nav.nav a[href="{name}"]').click()

    page.goto(BASE)
    expect(page.get_by_role("heading", level=1)).to_contain_text("Ask again")
    expect(page.locator("#spec .specimen")).to_be_visible()

    # a finished audit: the verdict sentence and one pen mark per try
    page.get_by_text("See a finished audit").click()
    expect(page.locator(".verdict")).to_contain_text("tries held")
    assert page.locator(".marks .mk").count() >= 10

    # the interface language picker is searchable and switches the page
    page.locator("#lang-btn").click()
    page.locator(".lm-search").fill("hindi")
    page.locator('.lang-menu [data-pick="hi"]').click()
    expect(page.get_by_text("एक असली नतीजा")).to_be_visible()
    page.locator("#lang-btn").click()
    page.locator('.lang-menu [data-pick="en"]').click()

    # translate: two panes and a language pair
    nav("#/translate")
    expect(page.locator("#in")).to_be_visible()
    expect(page.locator("#pick-dst")).to_be_visible()
    page.locator("#pick-dst").click()
    page.locator(".lm-search").fill("urdu")
    expect(page.locator('.lang-menu [data-pick="ur"]')).to_be_visible()
    page.keyboard.press("Escape")

    # Setu: nothing is spoken without consent in a sensitive setting
    nav("#/setu")
    expect(page.locator("[data-mic]")).to_have_count(2)
    page.select_option("#dm", "public_service")
    expect(page.locator("#agree")).to_be_visible()
    expect(page.locator("[data-mic=a]")).to_be_disabled()
    page.locator("#agree").click()
    expect(page.locator("[data-mic=a]")).to_be_enabled()

    # Learn: a lesson question can be answered and gives feedback
    nav("#/learn")
    page.locator("[data-lesson='0']").click()
    page.get_by_role("button", name="Yes, it changed").click()
    expect(page.locator("#sheet [role=status]")).to_be_visible()

    # Rules: the checker flags a sentence that is too long
    nav("#/rules")
    page.locator("#txt").fill(" ".join(["పదం"] * 20) + ".")
    page.get_by_role("button", name="Check it").click()
    expect(page.locator("#out .err-box")).to_contain_text("Sentences over the limit")

    assert not errors, errors
    print("ok")
    b.close()
