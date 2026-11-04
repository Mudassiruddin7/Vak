from __future__ import annotations

import json

import httpx

from vak.audit import perturb
from vak.audit.text_utils import (
    dominant_language,
    norm,
    numbers_in,
    script_share,
    squash,
    to_ascii_digits,
    to_native_digits,
)

TE = "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత?"
HI = "अगर एक किलो चावल 60 रुपये का है तो 3 किलो की क़ीमत क्या होगी?"


def test_norm_keeps_indic_words_whole():
    # \w-based cleanup would turn these into confetti because vowel signs are combining marks
    assert norm("బియ్యం, ధర!") == "బియ్యం ధర"
    assert norm("क़ीमत  क्या?") == "क़ीमत क्या"
    assert norm("<think>x</think>Hello,   WORLD") == "hello world"


def test_norm_ignores_digit_script_and_joiners():
    assert norm("౧౨౩ రూపాయలు") == norm("123 రూపాయలు")
    assert norm("क‍ा") == norm("का")


def test_squash_forgives_vowel_length():
    assert squash("బీయ్యం") == squash("బియ్యం")
    assert squash("కిలో") == squash("కిలొ")
    assert squash("ठीक") == squash("ठिक")
    assert squash("బియ్యం") != squash("ధర")


def test_digits_round_trip():
    assert to_native_digits("60 రూపాయలు", "te") == "౬౦ రూపాయలు"
    assert to_ascii_digits("౬౦ ६०") == "60 60"
    assert numbers_in("1,25,000 and ౬౦ and 3.5") == frozenset({"125000", "60", "3.5"})
    assert numbers_in("1,000 and 2,3,4") == frozenset({"1000", "2", "3", "4"})


def test_language_detection():
    assert dominant_language(TE) == "te"
    assert dominant_language(HI) == "hi"
    assert dominant_language("what is the capital?") == "en"
    assert script_share(TE, "te") > 0.9


def test_typos_are_repeatable_and_realistic():
    a = perturb.typo_variants(TE, 3, perturb.seeded_rng(TE))
    b = perturb.typo_variants(TE, 3, perturb.seeded_rng(TE))
    assert [v.text for v in a] == [v.text for v in b]
    assert len(a) >= 2
    for v in a:
        assert v.text != TE and dominant_language(v.text) == "te"
        # every Telugu combining mark must still follow a base letter
        assert not v.text.startswith(("ి", "ీ"))


def test_hindi_typos_touch_nukta_or_vowel_length():
    out = perturb.typo_variants(HI, 3, perturb.seeded_rng(HI))
    assert out and all(v.text != HI for v in out)


def test_latin_words_get_latin_slips():
    prompt = "Tell me the price of rice please"
    out = perturb.typo_variants(prompt, 2, perturb.seeded_rng(prompt))
    assert out and all(v.text != prompt for v in out)


def test_no_typo_possible_returns_nothing():
    assert perturb.typo_variants("12 34", 3, perturb.seeded_rng("x")) == []


def test_format_variants_do_not_change_meaning():
    for v in perturb.format_variants(TE, "te", 3):
        assert norm(v.text) == norm(TE)
        assert v.text != TE


def test_numeral_variant_switches_script_both_ways():
    v = perturb.numeral_variant(TE, "te")
    assert v and "౬౦" in v.text and norm(v.text) == norm(TE)
    back = perturb.numeral_variant(v.text, "te")
    assert back and "60" in back.text
    assert perturb.numeral_variant("ఎంత?", "te") is None


def test_distractor_polite_control_shapes():
    d = perturb.distractor_variants(TE, "te", 2)
    assert d[0].text.endswith(TE) and d[1].text.startswith(TE)
    assert all(TE in v.text for v in perturb.polite_variants(TE, "te", 2))
    c = perturb.control_variants(TE, "te", 2)
    assert all(v.kind == "sensitivity" and TE not in v.text for v in c)


async def test_paraphrases_are_filtered(clients):
    replies = {"rewrites": [TE, "బియ్యం కిలో 60 రూపాయలైతే మూడు కిలోలకు ఎంత అవుతుంది?", "x", "this is english only text here for sure"]}
    wire = clients(lambda req: httpx.Response(200, json={
        "choices": [{"message": {"content": json.dumps(replies, ensure_ascii=False)}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 5}}))
    out = await perturb.paraphrase_variants(wire.chat, "sarvam-105b", TE, "te", 3)
    assert [v.text for v in out] == [replies["rewrites"][1]]


async def test_language_variants_survive_one_failure(clients):
    def handler(req):
        body = json.loads(req.content)
        if body["target_language_code"] == "en-IN":
            return httpx.Response(500, json={"error": {"message": "down", "code": "internal_server_error"}})
        return httpx.Response(200, json={"translated_text": "अगर एक किलो चावल 60 रुपये का है तो 3 किलो?"})

    wire = clients(handler)
    out, problems = await perturb.language_variants(wire.text, TE, "te", 2)
    assert [v.id for v in out] == ["language-hi"]
    assert problems == ["couldn't translate into English"]
