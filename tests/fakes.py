"""A stand-in for api.sarvam.ai that is good enough to run whole audits offline."""
from __future__ import annotations

import json
import re
from collections import Counter

import httpx

from vak.audit.text_utils import norm

from .conftest import wav_b64

SAMPLE_EXPLANATION_TE = (
    "మీ మోడల్ అంటే జవాబులు ఇచ్చే కంప్యూటర్ భాగం. "
    "మేము దానిని చాలా విధాలుగా పరీక్షించాము. "
    "చిన్న అక్షర దోషాలు వచ్చినప్పుడు జవాబు కొంచెం మారింది. "
    "ఇది రైలు టికెట్ కౌంటర్ లాంటిది, అడిగే తీరు మారితే సమాధానం మారకూడదు. "
    "స్థిరత్వం వందలో డెబ్బై. "
    "ముందుగా ఇన్‌పుట్‌ను శుభ్రం చేయండి."
)


_LABELS = {
    "pos": {"సానుకూలం", "सकारात्मक", "positive"},
    "neg": {"ప్రతికూలం", "नकारात्मक", "negative"},
    "mix": {"మిశ్రమం", "मिला-जुला", "mixed"},
}


def _canon(text: str) -> str:
    t = norm(text)
    for key, words in _LABELS.items():
        if t in {norm(w) for w in words}:
            return key
    return t


def _grab(pattern: str, text: str) -> str:
    m = re.search(pattern, text, flags=re.DOTALL)
    return m.group(1) if m else ""


class FakeSarvam:
    """Call it like an httpx handler. Counts requests per path."""

    def __init__(self, *, explanation: str = SAMPLE_EXPLANATION_TE, judge_breaks: bool = False):
        self.explanation = explanation
        self.judge_breaks = judge_breaks
        self.paths: Counter = Counter()
        self.chat_bodies: list[dict] = []

    def __call__(self, request: httpx.Request) -> httpx.Response:
        path = request.url.path
        self.paths[path] += 1
        if path.endswith("/chat/completions"):
            body = json.loads(request.content)
            self.chat_bodies.append(body)
            return self._chat(body)
        if path == "/translate":
            body = json.loads(request.content)
            lang = body["target_language_code"]
            text = {"hi-IN": "यह फ़िल्म बहुत अच्छी है, कहानी शानदार है।", "en-IN": "This film is very good, the story is wonderful."}.get(lang, "x")
            return httpx.Response(200, json={"translated_text": text})
        if path == "/transliterate":
            return httpx.Response(200, json={"transliterated_text": "ee cinema chala bagundi katha adbhutam"})
        if path == "/text-to-speech":
            return httpx.Response(200, json={"audios": [wav_b64(2.0)]})
        if path == "/speech-to-text":
            return httpx.Response(200, json={"transcript": "ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.", "language_code": "te-IN"})
        return httpx.Response(404, json={"error": {"message": "no such route", "code": "not_found_error"}})

    # --- chat --------------------------------------------------------------------

    def _chat(self, body: dict) -> httpx.Response:
        messages = body["messages"]
        system = next((m["content"] for m in messages if m["role"] == "system"), "")
        user = messages[-1]["content"]
        n = body.get("n", 1)

        if system.startswith("You help a developer audit"):
            quoted = _grab(r"[\"“](.+?)[\"”]", user)
            return self._json({"prompt": quoted or user, "reference_output": None, "target_hint": None,
                               "audit_types": [], "depth": None, "language": "te"})
        if system.startswith("You rewrite a user's message"):
            return self._json({"rewrites": ["ఈ చిత్రం చాలా బాగుంది, కథ అద్భుతంగా ఉంది.", "ఈ సినిమా బాగా నచ్చింది, కథ చాలా బాగుంది.", "కథ అద్భుతం, సినిమా చాలా బాగుంది."]})
        if system.startswith("You write minimal counterfactuals"):
            return self._json({"rewrites": ["ఈ సినిమా చాలా చెత్త, కథ దారుణం."]})
        if system.startswith("You are a strict but fair grader"):
            if self.judge_breaks:
                return self._text("this is not json at all")
            reference = _canon(_grab(r"Reference answer: <<<(.*?)>>>", user))
            cands = re.findall(r"^\d+\. <<<(.*?)>>>$", user, flags=re.MULTILINE | re.DOTALL)
            rows = [{"i": i, "v": "same" if _canon(c) == reference else "different", "why": "fake judge"} for i, c in enumerate(cands, 1)]
            return self._json({"verdicts": rows})
        if system.startswith("You are an AI literacy educator"):
            return self._text(self.explanation)
        if "KEY:" in user:
            return self._text("సానుకూలం\nKEY: బాగుంది | అద్భుతం")
        return httpx.Response(200, json={"choices": [
            {"index": i, "finish_reason": "stop", "message": {"role": "assistant", "content": "సరే"}} for i in range(n)
        ], "usage": {"prompt_tokens": 20, "completion_tokens": 5}})

    @staticmethod
    def _text(text: str) -> httpx.Response:
        return httpx.Response(200, json={
            "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": text}}],
            "usage": {"prompt_tokens": 120, "completion_tokens": 80},
        })

    def _json(self, obj: dict) -> httpx.Response:
        return self._text(json.dumps(obj, ensure_ascii=False))
