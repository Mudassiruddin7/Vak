from __future__ import annotations

import json

import httpx
import pytest

from vak.audit.models import Faithfulness, Flag, Report, TestResult, Unit
from vak.explain import fallback, prompts, rules
from vak.explain.generator import Explainer

GOOD_TE = (
    "మోడల్ అంటే, ప్రశ్నలకు జవాబులు ఇచ్చే కంప్యూటర్ భాగం. "
    "మీ ప్రశ్నను చిన్న మార్పులతో మళ్ళీ మళ్ళీ అడిగి చూశాము. "
    "చిన్న అక్షర దోషాలు వస్తే జవాబు మారింది. "
    "ఇది రైలు స్టేషన్ ప్రకటనలో గోల వచ్చినా అర్థం మారకూడదు అన్నట్టు. "
    "స్థిరత్వానికి 100 కి 72 మార్కులు వచ్చాయి. "
    "ముందుగా ప్రశ్నను శుభ్రం చేసి పంపండి."
)


def make_report(score=0.72, flags=(), units=None, **kw) -> Report:
    base = dict(
        prompt="ఒక కిలో బియ్యం ధర ఎంత?", prompt_language="te", baseline_output="60 రూపాయలు",
        target_label="Sarvam 105B", depth="standard", audit_types=["consistency", "perturbation", "behavioral"],
        consistency_score=score, grade="mixed", total_perturbations=18, unexpected_changes=4,
        tests=[TestResult(id="t", category="typo", kind="invariance", label="x", input="x")],
        groups=[], units=units or [], faithfulness=Faithfulness(), flags=list(flags),
        strengths=["Asking again: held steady in 3 tests"], next_steps=["Clean the text first."],
        not_tested=["Very long inputs."],
    )
    base.update(kw)
    return Report(**base)


def flag(code, severity="medium") -> Flag:
    return Flag(code=code, severity=severity, title=code.replace("_", " ").capitalize(), detail=f"{code} detail")


ALL_CODES = list(fallback.TE["flags"])


# --- rules ----------------------------------------------------------------------------------

def test_banned_words_are_caught_in_all_three_spellings():
    assert rules.check("This uses a TOKEN here.", "te").banned == ["token"]
    assert "టోకెన్" in rules.check("ప్రతి టోకెన్లు లెక్కిస్తాము.", "te").banned
    assert "टोकन" in rules.check("हर टोकन गिना जाता है।", "hi").banned
    assert not rules.check(GOOD_TE, "te").banned


def test_zero_width_joiners_do_not_hide_a_banned_word():
    assert rules.check("న్యూరల్ నెట్‌వర్క్ వాడతాము.", "te").banned


def test_sentence_length_counts_words_not_characters():
    fifteen = " ".join(["పదం"] * 15) + "."
    sixteen = " ".join(["పదం"] * 16) + "."
    assert rules.check(fifteen + " " + GOOD_TE, "te").long_sentences == []
    assert rules.check(sixteen + " " + GOOD_TE, "te").long_sentences == [sixteen]


def test_script_and_length_checks():
    assert rules.check(GOOD_TE, "te").ok
    assert rules.check(GOOD_TE, "hi").wrong_script
    assert rules.check("Just English words, nothing else. Another sentence here. And a third. And a fourth.", "te").wrong_script
    assert rules.check("ఒకటి. రెండు.", "te").too_short
    assert rules.check(" ".join(["ఇది ఒక వాక్యం."] * 20), "te").too_long


def test_numbers_must_come_from_the_audit():
    report = make_report(score=0.72)          # 18 tests, 4 unexpected
    facts = rules.Facts.from_report(report)
    ok = rules.check(GOOD_TE, "te", facts)
    assert ok.ok, ok
    invented = rules.check(GOOD_TE.replace("72", "64").replace("తో", "తో 14 సార్లు"), "te", facts)
    assert invented.invented_numbers == ["14", "64"] and invented.missing_score
    assert "14" in invented.feedback() and "out of 100" in invented.feedback()
    # Telugu digits are read as digits, small counts and the audit's own numbers are fine
    assert rules.check("స్థిరత్వం ౭౨, మొత్తం 18, అనవసరంగా 4, మూడు 3.", "te", facts).invented_numbers == []


def test_clean_flattens_markdown_and_bullets():
    raw = "**ఫలితం:**\n- మొదటి విషయం\n- రెండవ విషయం 🙂\n1. మూడవది"
    assert rules.clean(raw) == "ఫలితం: మొదటి విషయం రెండవ విషయం మూడవది"


def test_split_long_cuts_at_commas_and_conjunctions():
    words = ["పదం"] * 9
    long = " ".join(words) + ", " + " ".join(words) + "."
    fixed = rules.split_long(long, "te")
    assert all(rules.word_count(s) <= 15 for s in rules.split_sentences(fixed))
    assert len(rules.split_sentences(fixed)) == 2
    hindi = "यह एक बहुत लंबा वाक्य है जो अपने आप में बहुत कुछ कहता है लेकिन फिर भी रुकता नहीं है और आगे बढ़ता ही जाता है।"
    assert all(rules.word_count(s) <= 15 for s in rules.split_sentences(rules.split_long(hindi, "hi")))


# --- the prompt ----------------------------------------------------------------------------

def test_prompt_keeps_the_brief_and_fills_every_placeholder():
    report = make_report(flags=[flag("surface_noise")], units=[Unit(index=0, text="బియ్యం", start=0, end=6, raw=0.9, importance=1.0)])
    for lang, word in (("te", "Telugu"), ("hi", "Hindi")):
        text = prompts.build_system(report, lang, "seed")
        assert "{" not in text and "}" not in text
        assert f"clear, plain {word}" in text
        assert "Maximum 15 words per sentence" in text
        assert "Consistency Score: 0.72 (higher is more consistent)" in text
        assert "Number of Perturbations Run: 18" in text
        assert "Number That Changed Output Unexpectedly: 4" in text
        assert "“బియ్యం”" in text


def test_nothing_we_feed_the_model_contains_a_banned_term():
    english_banned = rules.BANNED[:7]
    report = make_report(flags=[flag(c) for c in ALL_CODES])
    from vak.audit import scoring
    report.next_steps = scoring.next_steps(report.flags)
    fed = prompts.build_system(report, "te", "s").lower() + " " + " ".join(scoring._ADVICE.values()).lower()
    # the brief's own "Never use: ..." line has to name the banned terms, so look at everything else
    body = fed.split("critical rules:")[1].split("6. never use")[0] + fed.split("audit results summary:")[1]
    assert not [t for t in english_banned if t in body]


def test_analogy_depends_on_the_seed_only():
    r = make_report()
    assert prompts.build_system(r, "te", "a") == prompts.build_system(r, "te", "a")
    assert len({prompts.build_system(r, "te", str(i)) for i in range(12)}) > 1


# --- the fallback ----------------------------------------------------------------------------

@pytest.mark.parametrize("lang", ["te", "hi"])
@pytest.mark.parametrize("score", [0.3, 0.72, 0.93])
def test_every_fallback_explanation_obeys_the_rules(lang, score):
    units = [Unit(index=i, text=w, start=0, end=1, raw=0.9 if i == 1 else 0.0, importance=1.0 if i == 1 else 0.0)
             for i, w in enumerate(["ఒక", "బియ్యం", "ధర", "ఎంత"])]
    report = make_report(score=score, flags=[flag(c) for c in ALL_CODES[:3]], units=units)
    text = fallback.build(report, lang)
    found = rules.check(text, lang)
    assert not found.banned and not found.long_sentences and not found.wrong_script and not found.too_short


@pytest.mark.parametrize("lang", ["te", "hi"])
def test_each_flag_sentence_and_step_is_individually_valid(lang):
    bank = fallback.TE if lang == "te" else fallback.HI
    for sentence in [*bank["flags"].values(), *bank["steps"].values(), bank["model"], bank["what"], bank["next"],
                     *bank["concern"], *bank["mixed"], *bank["steady"]]:
        assert rules.word_count(sentence) <= 15, sentence
        assert not rules.check(sentence, lang).banned, sentence
    assert set(bank["flags"]) == set(ALL_CODES) == set(fallback.HI["flags"])
    assert set(bank["steps"]) - {"default"} == set(ALL_CODES)


def test_fallback_tone_follows_the_score():
    low = fallback.build(make_report(score=0.4), "te")
    high = fallback.build(make_report(score=0.95, unexpected_changes=0), "te")
    assert fallback.TE["concern"][0] in low and fallback.TE["steady"][0] not in low
    assert fallback.TE["steady"][0] in high and fallback.TE["unchanged"] in high


# --- the generator -------------------------------------------------------------------------

def chat_returning(*texts):
    queue = list(texts)
    seen: list[dict] = []

    def handler(req):
        body = json.loads(req.content)
        seen.append(body)
        text = queue.pop(0) if len(queue) > 1 else queue[0]
        return httpx.Response(200, json={"choices": [{"message": {"content": text}, "finish_reason": "stop"}],
                                         "usage": {"prompt_tokens": 100, "completion_tokens": 100}})

    handler.seen = seen
    return handler


async def test_clean_reply_is_used_as_is(clients, settings):
    wire = clients(chat_returning(GOOD_TE))
    out = await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
    assert out.source == "model" and out.attempts == 1
    assert out.text == GOOD_TE and len(out.sentences) == 6


async def test_invented_figures_are_treated_like_any_other_violation(clients, settings):
    handler = chat_returning(GOOD_TE.replace("72", "64"), GOOD_TE)
    wire = clients(handler)
    out = await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
    assert out.source == "model" and out.attempts == 2 and "72" in out.text
    assert "64" in handler.seen[1]["messages"][-1]["content"]


async def test_violations_are_fed_back_and_the_second_try_wins(clients, settings):
    bad = GOOD_TE + " ప్రతి టోకెన్ ముఖ్యం."
    handler = chat_returning(bad, GOOD_TE)
    wire = clients(handler)
    out = await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
    assert out.source == "model" and out.attempts == 2 and "టోకెన్" not in out.text
    retry = handler.seen[1]["messages"]
    assert retry[-1]["role"] == "user" and "not allowed" in retry[-1]["content"] and "టోకెన్" in retry[-1]["content"]


async def test_model_that_never_complies_gets_replaced_by_the_template(clients, settings):
    wire = clients(chat_returning("This is all English and breaks every rule."))
    out = await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
    assert out.source == "template" and out.attempts == 3
    assert rules.check(out.text, "te").banned == []
    assert any("prepared explanation" in n for n in out.notes)


async def test_long_sentences_are_repaired_not_rejected(clients, settings):
    long = ("మోడల్ అంటే జవాబులు ఇచ్చే భాగం. " + " ".join(["పదం"] * 9) + ", " + " ".join(["పదం"] * 9) + ". "
            "రెండో విషయం ఇది. మూడో విషయం ఇది. నాలుగో విషయం ఇది.")
    wire = clients(chat_returning(long))
    out = await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
    assert out.source == "model" and out.attempts == 3
    assert all(rules.word_count(s) <= 15 for s in out.sentences)


async def test_api_trouble_falls_back_but_credit_problems_surface(clients, settings):
    from vak.sarvam import OutOfCredits

    wire = clients(lambda req: httpx.Response(400, json={"error": {"message": "nope", "code": "invalid_request_error"}}))
    out = await Explainer(wire.chat, settings).explain(make_report(), "hi", seed="x")
    assert out.source == "template" and rules.check(out.text, "hi").ok is not None

    wire = clients(lambda req: httpx.Response(402, json={"error": {"message": "No credits available.", "code": "insufficient_quota_error"}}))
    with pytest.raises(OutOfCredits):
        await Explainer(wire.chat, settings).explain(make_report(), "te", seed="x")
