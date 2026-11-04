"""The writing rules from the brief, enforced in code instead of hoped for.

Banned words, 15 words per sentence, the right script, plain text only. The model
gets its own violations fed back to it; whatever survives is repaired or replaced
by a template.
"""
from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass, field

from ..audit.models import Report
from ..audit.text_utils import numbers_in, script_counts, script_share

MAX_WORDS = 15
MIN_SENTENCES = 4
MAX_SENTENCES = 14
SMALL_NUMBERS = frozenset(str(n) for n in range(0, 11))   # "3 steps", "2 times" are fine

# English terms, then how they get spelt out in Telugu and Devanagari. Substring match, so plurals are covered.
BANNED = [
    "transformer", "embedding", "attention", "neural network", "neural net", "logit", "token",
    "ట్రాన్స్ఫార్మర్", "ఎంబెడ్డింగ్", "ఎంబెడింగ్", "అటెన్షన్", "న్యూరల్ నెట్వర్క్", "న్యూరల్ నెట్ వర్క్", "లాజిట్", "టోకెన్",
    "ट्रांसफॉर्मर", "ट्रांसफ़ॉर्मर", "एम्बेडिंग", "एंबेडिंग", "अटेंशन", "अटेन्शन", "न्यूरल नेटवर्क", "न्यूरल नेट वर्क", "लॉजिट", "टोकन",
]

_SENTENCE_SPLIT = re.compile(r"(?<=[.?!।॥])\s+|\n+")
_MARKUP = re.compile(r"[*_#`>~|]+")
_BULLET = re.compile(r"^\s*(?:[-•●▪]|\d+[.)])\s+", re.MULTILINE)
_EMOJI = re.compile("[\U0001F300-\U0001FAFF\U00002600-\U000027BF\U0001F000-\U0001F2FF️]")
_JOINERS = re.compile("[​-‍⁠﻿]")


def _plain(text: str) -> str:
    """Text with joiners removed and case folded, for substring checks."""
    return _JOINERS.sub("", unicodedata.normalize("NFKC", text)).casefold()


def clean(text: str) -> str:
    """Strip markup, bullets and emoji; fold the reply into one tidy paragraph."""
    text = _EMOJI.sub("", text)
    text = _BULLET.sub("", text)
    text = _MARKUP.sub("", text)
    text = re.sub(r"\s*\n\s*", " ", text)
    return re.sub(r"[ \t]{2,}", " ", text).strip()


def split_sentences(text: str) -> list[str]:
    return [s.strip() for s in _SENTENCE_SPLIT.split(text) if s and s.strip()]


def word_count(sentence: str) -> int:
    return sum(1 for w in sentence.split() if any(ch.isalnum() or unicodedata.category(ch) in ("Mn", "Mc") for ch in w))


@dataclass(frozen=True)
class Facts:
    """What the explanation is allowed to say numerically. Anything else counts as made up."""
    score_100: int
    allowed: frozenset[str]

    @classmethod
    def from_report(cls, report: Report) -> "Facts":
        score = report.consistency_score
        nums = {str(round(score * 100)), f"{score:.2f}", str(score), "100",
                str(report.total_perturbations), str(report.unexpected_changes), str(report.skipped)}
        nums |= numbers_in(report.target_label) | numbers_in(report.prompt) | numbers_in(report.baseline_output)
        text = " ".join([*report.not_tested, *report.next_steps, *report.strengths, *(f.detail for f in report.flags)])
        nums |= numbers_in(text)
        return cls(round(score * 100), frozenset(nums) | SMALL_NUMBERS)


@dataclass
class Violations:
    banned: list[str] = field(default_factory=list)
    long_sentences: list[str] = field(default_factory=list)
    wrong_script: bool = False
    latin_heavy: bool = False
    sentence_count: int = 0
    too_short: bool = False
    too_long: bool = False
    invented_numbers: list[str] = field(default_factory=list)
    missing_score: bool = False

    @property
    def ok(self) -> bool:
        return not (self.banned or self.long_sentences or self.wrong_script or self.latin_heavy or self.too_short
                    or self.too_long or self.invented_numbers or self.missing_score)

    @property
    def severity(self) -> tuple:
        """Lower is better; used to pick the best of several attempts."""
        return (bool(self.banned) + self.wrong_script + self.too_short + bool(self.invented_numbers),
                len(self.banned) + len(self.invented_numbers), self.latin_heavy + self.too_long + self.missing_score,
                len(self.long_sentences))

    def feedback(self) -> str:
        lines = []
        if self.banned:
            lines.append("You used words that are not allowed: " + ", ".join(self.banned) + ". Describe the idea in plain everyday words instead.")
        if self.long_sentences:
            shown = " / ".join(s if len(s) < 120 else s[:117] + "..." for s in self.long_sentences[:3])
            lines.append(f"These sentences are longer than {MAX_WORDS} words. Split or shorten them: {shown}")
        if self.wrong_script or self.latin_heavy:
            lines.append("Write only in the requested script. Do not use English letters or English words (digits are fine).")
        if self.too_short:
            lines.append("The explanation is too short. Write 8 to 12 sentences covering what was found, what it means and what to do next.")
        if self.too_long:
            lines.append("The explanation is too long. Keep it to 12 sentences.")
        if self.invented_numbers:
            lines.append("You used numbers that are not in the audit: " + ", ".join(self.invented_numbers)
                         + ". Use only the numbers you were given.")
        if self.missing_score:
            lines.append("Say the consistency score as a number out of 100, written in digits.")
        return "\n".join(lines)


def check(text: str, lang: str, facts: Facts | None = None) -> Violations:
    plain = _plain(text)
    sentences = split_sentences(text)
    v = Violations(sentence_count=len(sentences))
    v.banned = sorted({term for term in BANNED if _plain(term) in plain})
    v.long_sentences = [s for s in sentences if word_count(s) > MAX_WORDS]
    counts = script_counts(text)
    total = sum(counts.values())
    v.wrong_script = total == 0 or script_share(text, lang) < 0.6
    v.latin_heavy = total > 0 and counts["en"] / total > 0.08
    v.too_short = len(sentences) < MIN_SENTENCES
    v.too_long = len(sentences) > MAX_SENTENCES
    if facts is not None:
        used = numbers_in(text)
        v.invented_numbers = sorted(n for n in used if n not in facts.allowed)
        v.missing_score = str(facts.score_100) not in used
    return v


_SPLIT_WORDS = {
    "te": ("కానీ", "అయితే", "మరియు", "కాబట్టి", "అందువల్ల", "లేదా"),
    "hi": ("लेकिन", "मगर", "और", "इसलिए", "या", "जबकि"),
}


def split_long(text: str, lang: str, limit: int = MAX_WORDS) -> str:
    """Last-resort repair: cut over-long sentences at a comma or conjunction near the middle."""
    out = []
    for sentence in split_sentences(text):
        out.extend(_cut(sentence, lang, limit))
    return " ".join(out)


def _cut(sentence: str, lang: str, limit: int) -> list[str]:
    words = sentence.split()
    if len(words) <= limit:
        return [sentence]
    end = sentence[-1] if sentence[-1] in ".?!।॥" else "।" if lang == "hi" else "."
    body = sentence.rstrip(".?!।॥")
    words = body.split()
    candidates = []
    for i, w in enumerate(words[:-1]):
        if 3 <= i + 1 <= len(words) - 3 and (w.endswith((",", "،", ";")) or words[i + 1] in _SPLIT_WORDS.get(lang, ())):
            candidates.append(i + 1)
    if not candidates:
        return [sentence]
    cut = min(candidates, key=lambda i: abs(i - len(words) / 2))
    first = " ".join(words[:cut]).rstrip(",;،")
    rest = " ".join(words[cut:])
    stop = "।" if lang == "hi" else "."
    return _cut(first + stop, lang, limit) + _cut(rest + end, lang, limit)
