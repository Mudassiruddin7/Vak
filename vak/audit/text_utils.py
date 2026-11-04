"""Script-aware text helpers.

A note on why this isn't just regexes: Python's \\w does not match Indic vowel
signs (they are combining marks), so "strip punctuation with [^\\w\\s]" quietly
chops Telugu and Hindi words to pieces. Everything here goes by Unicode category.
"""
from __future__ import annotations

import re
import unicodedata
from collections import Counter
from dataclasses import dataclass

from ..util import strip_think

TELUGU_RANGE = (0x0C00, 0x0C7F)
DEVANAGARI_RANGE = (0x0900, 0x097F)
TELUGU_DIGIT_ZERO = 0x0C66
DEVANAGARI_DIGIT_ZERO = 0x0966


def script_of(ch: str) -> str | None:
    """'te', 'hi' (Devanagari), 'en' (Latin) or None for anything else."""
    cp = ord(ch)
    if TELUGU_RANGE[0] <= cp <= TELUGU_RANGE[1]:
        return "te"
    if DEVANAGARI_RANGE[0] <= cp <= DEVANAGARI_RANGE[1]:
        return "hi"
    if ch.isascii() and ch.isalpha():
        return "en"
    if unicodedata.name(ch, "").startswith("LATIN"):
        return "en"
    return None


def script_counts(text: str) -> Counter:
    counts: Counter = Counter()
    for ch in text:
        if ch.isalpha() or unicodedata.category(ch) in ("Mn", "Mc"):
            s = script_of(ch)
            if s:
                counts[s] += 1
    return counts


def dominant_language(text: str, default: str = "en") -> str:
    counts = script_counts(text)
    if not counts:
        return default
    lang, _ = counts.most_common(1)[0]
    return lang


def script_share(text: str, lang: str) -> float:
    counts = script_counts(text)
    total = sum(counts.values())
    return counts[lang] / total if total else 0.0


def to_ascii_digits(text: str) -> str:
    out = []
    for ch in text:
        if unicodedata.category(ch) == "Nd":
            out.append(str(unicodedata.digit(ch)))
        else:
            out.append(ch)
    return "".join(out)


def to_native_digits(text: str, lang: str) -> str:
    zero = {"te": TELUGU_DIGIT_ZERO, "hi": DEVANAGARI_DIGIT_ZERO}.get(lang)
    if zero is None:
        return text
    return "".join(chr(zero + int(ch)) if "0" <= ch <= "9" else ch for ch in text)


def has_ascii_digits(text: str) -> bool:
    return any("0" <= ch <= "9" for ch in text)


def has_native_digits(text: str) -> bool:
    return any(unicodedata.category(ch) == "Nd" and not ch.isascii() for ch in text)


def norm(text: str) -> str:
    """Comparison form: no markup, case, digit script, punctuation, joiners or extra spaces."""
    text = strip_think(text or "")
    text = unicodedata.normalize("NFKC", text).casefold()
    text = to_ascii_digits(text)
    kept = []
    for ch in text:
        cat = unicodedata.category(ch)
        if cat == "Cf":                      # zero-width joiners and friends
            continue
        if cat[0] in "PSZC" or ch.isspace():
            kept.append(" ")
        else:
            kept.append(ch)
    return re.sub(r"\s+", " ", "".join(kept)).strip()


def squash(text: str) -> str:
    """Looser than norm(): also forgets vowel length, nukta and nasal marks, for spotting 'same word, spelt differently'."""
    t = norm(text)
    table = {
        "ీ": "ి", "ూ": "ు", "ే": "ె", "ో": "ొ",          # Telugu long vowel signs -> short
        "ी": "ि", "ू": "ु",                                # Devanagari
        "ँ": "ं", "ః": "",
        "़": "",                                      # nukta
    }
    return "".join(table.get(ch, ch) for ch in t)


def numbers_in(text: str) -> frozenset[str]:
    """Digit runs in the text, thousands commas removed, in ASCII digits."""
    t = to_ascii_digits(text or "")
    # western (1,000) and Indian (1,25,000) grouping both collapse; "1,2,3" stays a list
    t = re.sub(r"(?<=\d),(?=\d{2,3}(?!\d))", "", t)
    return frozenset(re.findall(r"\d+(?:\.\d+)?", t))


@dataclass(frozen=True)
class Span:
    text: str
    start: int
    end: int


def word_spans(text: str) -> list[Span]:
    return [Span(m.group(), m.start(), m.end()) for m in re.finditer(r"\S+", text)]


def tidy_spaces(text: str) -> str:
    return re.sub(r"[ \t]+", " ", re.sub(r"[ \t]*\n[ \t]*", "\n", text)).strip()


def trim_punct(word: str) -> str:
    """The word without punctuation glued to its edges."""
    start, end = 0, len(word)
    while start < end and unicodedata.category(word[start])[0] in "PS":
        start += 1
    while end > start and unicodedata.category(word[end - 1])[0] in "PS":
        end -= 1
    return word[start:end]
