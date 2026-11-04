"""The perturbation suite: ways to change a prompt that should (or should not) change the answer.

Meaning-preserving edits ("invariance") should leave the answer alone: spelling slips,
spacing, digit scripts, Roman-letter typing, rewordings, other languages, side remarks.
Meaning-changing edits ("sensitivity") should move it: a flipped question, an unrelated one.

Spelling slips are the kind real Telugu and Hindi typists make (short/long vowel
mix-ups, a dropped anusvara or nukta), not random character noise.
"""
from __future__ import annotations

import asyncio
import random
import re
from dataclasses import dataclass

from ..catalog import LANGS
from ..sarvam import ChatClient, TextClient
from ..util import digest, extract_json
from .text_utils import (
    dominant_language,
    has_ascii_digits,
    has_native_digits,
    norm,
    script_share,
    tidy_spaces,
    to_ascii_digits,
    to_native_digits,
    trim_punct,
    word_spans,
)


@dataclass
class Variant:
    id: str
    category: str
    kind: str          # "invariance" or "sensitivity"
    label: str
    text: str
    note: str = ""


# --- tables -------------------------------------------------------------------

_TE_SWAPS = [
    ("ి", "ీ"), ("ీ", "ి"),   # ి ీ
    ("ు", "ూ"), ("ూ", "ు"),   # ు ూ
    ("ె", "ే"), ("ే", "ె"),   # ె ే
    ("ొ", "ో"), ("ో", "ొ"),   # ొ ో
    ("ల", "ళ"),                         # ల -> ళ
    ("న", "ణ"),                         # న -> ణ
]
_TE_DROPS = ["ం"]                            # anusvara ం

_HI_SWAPS = [
    ("ि", "ी"), ("ी", "ि"),   # ि ी
    ("ु", "ू"), ("ू", "ु"),   # ु ू
    ("ं", "ँ"), ("ँ", "ं"),   # ं ँ
    ("श", "ष"),                         # श -> ष
    ("ण", "न"),                         # ण -> न
]
_HI_DROPS = ["़", "ं"]                  # nukta, anusvara

DISTRACTORS = {
    "te": ["ఈ రోజు ఉదయం కొంచెం వర్షం పడింది.", "నిన్న నేను ఒక కొత్త పుస్తకం కొన్నాను.", "మా ఆఫీసు రెండో అంతస్తులో ఉంది."],
    "hi": ["आज सुबह थोड़ी बारिश हुई थी।", "कल मैंने एक नई किताब खरीदी।", "हमारा दफ़्तर दूसरी मंज़िल पर है।"],
    "en": ["It rained a little this morning.", "I bought a new notebook yesterday.", "Our office is on the second floor."],
}
POLITE = {
    "te": [("దయచేసి చెప్పండి: ", ""), ("", " ధన్యవాదాలు.")],
    "hi": [("कृपया बताइए: ", ""), ("", " धन्यवाद।")],
    "en": [("Please tell me: ", ""), ("", " Thank you.")],
}
CONTROLS = {
    "te": ["భారతదేశ రాజధాని ఏది?", "చపాతీ ఎలా చేయాలి?"],
    "hi": ["भारत की राजधानी क्या है?", "रोटी कैसे बनाते हैं?"],
    "en": ["What is the capital of India?", "How do I make a cup of tea?"],
}
LANGUAGE_ORDER = {"te": ["hi", "en"], "hi": ["te", "en"], "en": ["te", "hi"]}


def seeded_rng(prompt: str) -> random.Random:
    """Same prompt, same edits. Keeps audits repeatable and the answer cache useful."""
    return random.Random(int(digest("perturb", prompt)[:12], 16))


# --- spelling slips ----------------------------------------------------------------

def _indic_options(word: str, lang: str) -> list[tuple[str, str]]:
    swaps, drops = (_TE_SWAPS, _TE_DROPS) if lang == "te" else (_HI_SWAPS, _HI_DROPS)
    options: list[tuple[str, str]] = []
    for a, b in swaps:
        for m in re.finditer(re.escape(a), word):
            options.append((word[: m.start()] + b + word[m.end():], f"{a}→{b}"))
    for ch in drops:
        for m in re.finditer(re.escape(ch), word):
            options.append((word[: m.start()] + word[m.end():], f"dropped {ch}"))
    return options


def _latin_options(word: str) -> list[tuple[str, str]]:
    if len(word) < 4 or not word.isalpha():
        return []
    options = []
    for i in range(1, len(word) - 2):                     # keep first and last letters
        if word[i] != word[i + 1]:
            options.append((word[:i] + word[i + 1] + word[i] + word[i + 2:], "swapped two letters"))
    for i in range(1, len(word) - 1):
        options.append((word[:i] + word[i + 1:], "missing letter"))
    return options


def typo_variants(prompt: str, count: int, rng: random.Random) -> list[Variant]:
    pools = []
    for sp in word_spans(prompt):
        core = trim_punct(sp.text)
        if len(core) < 3:
            continue
        script = dominant_language(core)
        options = _latin_options(core) if script == "en" else _indic_options(core, script) if script in ("te", "hi") else []
        if options:
            pools.append((sp, core, options))
    if not pools:
        return []
    out: list[Variant] = []
    seen = {prompt}
    for attempt in range(60):
        if len(out) >= count:
            break
        slips = 1 if not out else rng.choice([1, 2])
        picks = rng.sample(pools, min(slips, len(pools)))
        text, notes = prompt, []
        for sp, core, options in sorted(picks, key=lambda p: -p[0].start):
            new_core, why = rng.choice(options)
            text = text[: sp.start] + sp.text.replace(core, new_core, 1) + text[sp.end:]
            notes.append(f"{core} → {new_core} ({why})")
        if text not in seen:
            seen.add(text)
            n = len(picks)
            out.append(Variant(
                f"typo-{len(out) + 1}", "typo", "invariance",
                "A small spelling slip" if n == 1 else "Two small spelling slips", text, "; ".join(reversed(notes)),
            ))
    return out


# --- formatting --------------------------------------------------------------------

def format_variants(prompt: str, lang: str, count: int) -> list[Variant]:
    words = prompt.split()
    cands: list[tuple[str, str]] = []
    if len(words) >= 2:
        cands.append(("Extra spaces between words", "  ".join(words)))
    tail = prompt.rstrip(" ?.!।")
    if tail != prompt.rstrip():
        cands.append(("Question mark or full stop removed", tail))
    else:
        cands.append(("A full stop added at the end", prompt.rstrip() + ("।" if lang == "hi" else ".")))
    latin = sum(ch.isascii() and ch.isalpha() for ch in prompt)
    if latin >= 3:
        cands.append(("All lowercase", prompt.lower()))
        cands.append(("ALL CAPITALS", prompt.upper()))
    if len(words) >= 4:
        mid = len(words) // 2
        cands.append(("A line break in the middle", " ".join(words[:mid]) + "\n" + " ".join(words[mid:])))
    if lang == "hi" and "।" in prompt:
        cands.append(("Full stop instead of danda", prompt.replace("।", ".")))
    out, seen = [], {prompt, tidy_spaces(prompt)}
    for label, text in cands:
        if text in seen or norm(text) != norm(text.strip()):
            continue
        seen.add(text)
        out.append(Variant(f"format-{len(out) + 1}", "format", "invariance", label, text))
        if len(out) >= count:
            break
    return out


def numeral_variant(prompt: str, lang: str) -> Variant | None:
    if lang in ("te", "hi") and has_ascii_digits(prompt):
        return Variant("numerals-1", "numerals", "invariance",
                       f"Digits written as {LANGS[lang].english} numerals", to_native_digits(prompt, lang))
    if has_native_digits(prompt):
        return Variant("numerals-1", "numerals", "invariance", "Digits written as 0-9", to_ascii_digits(prompt))
    return None


def distractor_variants(prompt: str, lang: str, count: int) -> list[Variant]:
    pool = DISTRACTORS.get(lang, DISTRACTORS["en"])
    out = []
    for i, line in enumerate(pool[:count]):
        text = f"{line} {prompt}" if i % 2 == 0 else f"{prompt} {line}"
        where = "before" if i % 2 == 0 else "after"
        out.append(Variant(f"distractor-{i + 1}", "distractor", "invariance",
                           f"An unrelated remark {where} the question", text, line))
    return out


def polite_variants(prompt: str, lang: str, count: int) -> list[Variant]:
    out = []
    for i, (pre, post) in enumerate(POLITE.get(lang, POLITE["en"])[:count]):
        label = "Politely worded" if pre else "A thank-you added"
        out.append(Variant(f"polite-{i + 1}", "polite", "invariance", label, f"{pre}{prompt}{post}".strip()))
    return out


def control_variants(prompt: str, lang: str, count: int) -> list[Variant]:
    out = []
    for i, text in enumerate(CONTROLS.get(lang, CONTROLS["en"])[:count]):
        out.append(Variant(f"control-{i + 1}", "control", "sensitivity",
                           "A completely different question", text, "sanity check: the answer should change"))
    return out


# --- generated variants (language model, translator) ------------------------------------

_REWRITE_SYSTEM = {
    "paraphrase": (
        "You rewrite a user's message without changing what it asks. Keep the same language and script as the original. "
        "Write {n} different rewrites. Each must keep every fact, number and name, and ask for exactly the same thing. "
        "Change wording and sentence structure, not meaning. Do not answer the message.\n"
        'Reply with JSON only: {{"rewrites": ["...", "..."]}}'
    ),
    "opposite": (
        "You write minimal counterfactuals. Rewrite the user's message so that the correct answer would be different: "
        "flip the key fact, add a negation, or reverse the sentiment or condition, changing as few words as possible. "
        "Keep the same language and script. Write {n} different versions. Do not answer the message.\n"
        'Reply with JSON only: {{"rewrites": ["...", "..."]}}'
    ),
}


async def _rewrites(chat: ChatClient, model: str, prompt: str, lang: str, n: int, mode: str) -> list[str]:
    reply = await chat.complete(
        model,
        [{"role": "system", "content": _REWRITE_SYSTEM[mode].format(n=n)}, {"role": "user", "content": prompt}],
        temperature=0.0, max_tokens=700, json_mode=True,
    )
    data = extract_json(reply.text)
    items = data.get("rewrites") if isinstance(data, dict) else data if isinstance(data, list) else None
    if not isinstance(items, list):
        return []
    base = norm(prompt)
    out: list[str] = []
    for item in items:
        if not isinstance(item, str):
            continue
        item = item.strip()
        if not item or norm(item) == base or any(norm(item) == norm(o) for o in out):
            continue
        if not (0.4 * len(prompt) <= len(item) <= 2.6 * len(prompt) + 20):
            continue
        if lang in ("te", "hi") and script_share(item, lang) < 0.5:
            continue
        out.append(item)
    return out[:n]


async def paraphrase_variants(chat: ChatClient, model: str, prompt: str, lang: str, count: int) -> list[Variant]:
    if count <= 0:
        return []
    texts = await _rewrites(chat, model, prompt, lang, count, "paraphrase")
    return [Variant(f"paraphrase-{i + 1}", "paraphrase", "invariance", "Said in different words", t) for i, t in enumerate(texts)]


async def opposite_variants(chat: ChatClient, model: str, prompt: str, lang: str, count: int) -> list[Variant]:
    if count <= 0:
        return []
    texts = await _rewrites(chat, model, prompt, lang, count, "opposite")
    return [Variant(f"negation-{i + 1}", "negation", "sensitivity", "Meaning flipped on purpose", t,
                    "sanity check: the answer should change") for i, t in enumerate(texts)]


async def roman_variant(text: TextClient, prompt: str, lang: str) -> Variant | None:
    if lang not in ("te", "hi"):
        return None
    typed = await text.transliterate(prompt, lang, "en")
    if norm(typed) == norm(prompt):
        return None
    return Variant("script-1", "script", "invariance", "Typed in English letters, like on a phone", typed)


async def language_variants(text: TextClient, prompt: str, lang: str, count: int) -> tuple[list[Variant], list[str]]:
    targets = LANGUAGE_ORDER.get(lang, ["en"])[:count]
    results = await asyncio.gather(*(text.translate(prompt, lang, dst) for dst in targets), return_exceptions=True)
    out, problems = [], []
    for dst, res in zip(targets, results):
        if isinstance(res, Exception):
            problems.append(f"couldn't translate into {LANGS[dst].english}")
            continue
        if norm(res) == norm(prompt):
            continue
        out.append(Variant(f"language-{dst}", "language", "invariance", f"Asked in {LANGS[dst].english}", res))
    return out, problems
