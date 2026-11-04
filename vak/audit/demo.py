"""Two tiny stand-in models so the app has something to audit without spending anything.

"Mood Meter" reads a short review in Telugu, Hindi or English and says whether it is
positive, negative or mixed. The steady version cleans its input and notices
negation. The shaky one is the sort of code people really write: exact word
matches, punctuation glued to words, first hit wins, negation ignored, and only
the first few dozen characters read.
"""
from __future__ import annotations

from .targets import Answer, Target
from .text_utils import dominant_language, norm, squash

POSITIVE = {
    "te": ["బాగుంది", "బావుంది", "బాగున్నాయి", "బాగా", "అద్భుతం", "సూపర్", "నచ్చింది", "ఇష్టం", "మంచి", "గొప్ప", "అదిరింది"],
    "hi": ["अच्छा", "अच्छी", "बढ़िया", "शानदार", "पसंद", "बेहतरीन", "खूब", "मज़ेदार", "कमाल"],
    "en": ["good", "great", "love", "loved", "excellent", "amazing", "nice", "wonderful", "fantastic", "awesome", "superb"],
}
NEGATIVE = {
    "te": ["చెత్త", "బాగాలేదు", "బాగోలేదు", "నిరాశ", "చెడ్డ", "బోరు", "విసుగు", "దారుణం", "నచ్చలేదు"],
    "hi": ["बुरा", "बुरी", "खराब", "बेकार", "निराश", "बोरिंग", "घटिया", "नापसंद", "बकवास"],
    "en": ["bad", "awful", "hate", "hated", "boring", "terrible", "worst", "poor", "disappointing", "dull", "waste"],
}
NEGATORS = {
    "te": ["కాదు", "లేదు", "లేవు"],
    "hi": ["नहीं", "ना"],
    "en": ["not", "no", "never", "isnt", "wasnt", "dont", "didnt"],
}
LABELS = {
    "te": {"pos": "సానుకూలం", "neg": "ప్రతికూలం", "mix": "మిశ్రమం"},
    "hi": {"pos": "सकारात्मक", "neg": "नकारात्मक", "mix": "मिला-जुला"},
    "en": {"pos": "positive", "neg": "negative", "mix": "mixed"},
}

SHAKY_WINDOW = 42

DEMOS = {
    "mood-steady": "Mood Meter (steady)",
    "mood-shaky": "Mood Meter (shaky)",
}


def _stem_match(word: str, lexicon: list[str]) -> bool:
    w = squash(word)
    return any(len(s) >= 3 and (w == s or w.startswith(s)) for s in map(squash, lexicon))


class MoodMeter(Target):
    billed = False
    can_explain = False

    def __init__(self, steady: bool):
        self.steady = steady
        self.label = DEMOS["mood-steady" if steady else "mood-shaky"]

    def _verdict(self, text: str) -> str:
        lang = dominant_language(text, "en")
        if lang not in LABELS:
            lang = "en"
        return LABELS[lang][self._polarity(text, lang)]

    def _polarity(self, text: str, lang: str) -> str:
        if self.steady:
            words = norm(text).replace("'", "").split()
            score = 0
            for i, w in enumerate(words):
                sign = 1 if _stem_match(w, POSITIVE[lang]) else -1 if _stem_match(w, NEGATIVE[lang]) else 0
                if sign and any(_stem_match(x, NEGATORS[lang]) for x in words[i + 1:i + 3]):
                    sign = -sign
                score += sign
            return "pos" if score > 0 else "neg" if score < 0 else "mix"
        for w in text[:SHAKY_WINDOW].lower().split():   # only reads the start of its input
            w = w.strip(".?!")
            if w in POSITIVE[lang]:
                return "pos"
            if w in NEGATIVE[lang]:
                return "neg"
        return "mix"

    async def ask(self, prompt, *, temperature=0.0, n=1, max_tokens=420):
        return [Answer(self._verdict(prompt)) for _ in range(n)]


def build_demo(model_id: str) -> Target:
    if model_id not in DEMOS:
        raise ValueError(f"unknown demo model {model_id!r}")
    return MoodMeter(steady=model_id == "mood-steady")
