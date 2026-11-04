"""Explanation Generator: asks the language model for a plain-language write-up and
holds it to the brief's rules. If it can't comply after a few tries, a hand-written
template takes over, so the person always gets something readable.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass, field

from ..audit.models import Report
from ..config import Settings
from ..sarvam import AuthError, ChatClient, OutOfCredits, SarvamError
from . import fallback, prompts, rules

MAX_ATTEMPTS = 3
MAX_SENTENCES_KEPT = 14


@dataclass
class Explanation:
    language: str
    text: str
    sentences: list[str]
    source: str                 # "model" or "template"
    attempts: int
    model: str
    notes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


def _from_text(language: str, text: str, source: str, attempts: int, model: str, notes: list[str]) -> Explanation:
    return Explanation(language, text, rules.split_sentences(text), source, attempts, model, notes)


class Explainer:
    def __init__(self, chat: ChatClient, settings: Settings):
        self.chat = chat
        self.model = settings.explain_model

    async def explain(self, report: Report, language: str, *, seed: str) -> Explanation:
        messages = [
            {"role": "system", "content": prompts.build_system(report, language, seed)},
            {"role": "user", "content": prompts.build_user(language)},
        ]
        best: tuple[str, rules.Violations] | None = None
        notes: list[str] = []
        attempts = 0
        for attempts in range(1, MAX_ATTEMPTS + 1):
            try:
                reply = await self.chat.complete(self.model, messages, temperature=0.4, max_tokens=1500, use_cache=False)
            except (AuthError, OutOfCredits):
                raise
            except SarvamError as exc:
                notes.append(f"The language model was unavailable ({exc.message[:80]}).")
                break
            text = rules.clean(reply.text)
            found = rules.check(text, language)
            if best is None or found.severity < best[1].severity:
                best = (text, found)
            if found.ok:
                break
            messages += [{"role": "assistant", "content": reply.text}, {"role": "user", "content": found.feedback()}]

        if best is not None:
            text, found = best
            if found.long_sentences:
                text = rules.split_long(text, language)
                found = rules.check(text, language)
            if found.too_long:
                text = " ".join(rules.split_sentences(text)[:MAX_SENTENCES_KEPT])
                found = rules.check(text, language)
            if not (found.banned or found.wrong_script or found.latin_heavy or found.too_short):
                if found.long_sentences:
                    notes.append("A few sentences are a little longer than 15 words.")
                return _from_text(language, text, "model", attempts, self.model, notes)
            notes.append("The model's wording kept breaking the writing rules, so a prepared explanation is shown.")

        return _from_text(language, fallback.build(report, language), "template", attempts, self.model, notes)
