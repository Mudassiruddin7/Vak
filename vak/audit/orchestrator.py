"""Audit Orchestrator: turns what the developer said or typed into a concrete plan.

The language model reads the message, but it is never trusted with the prompt
itself. If the text it hands back isn't really in the developer's message, we fall
back to a quoted passage or to the whole message instead.
"""
from __future__ import annotations

import re

from ..config import Settings
from ..sarvam import AuthError, ChatClient, OutOfCredits
from ..util import extract_json
from .models import ALL_TYPES, Plan, TargetSpec
from .text_utils import dominant_language, squash

SYSTEM = (
    "You help a developer audit an AI model. They speak or type in Telugu, Hindi, English or a mix of these.\n"
    "Read their message and pull out:\n"
    '- "prompt": the exact text they want to test the model with, copied word for word from their message, in the same '
    "language and script. If they quoted it, use the quote. If the whole message is just the text to test, return all of it.\n"
    '- "reference_output": an answer the model gave that they want checked, copied exactly, or null.\n'
    '- "target_hint": the name of a model if they mention one (for example Gemma, DeepSeek, Sarvam), otherwise null.\n'
    '- "audit_types": which checks they asked for, any of "consistency" (same answer when asked again), '
    '"perturbation" (small changes such as typos, rewording, other languages) and "behavioral" (sensible reaction when '
    "the meaning changes). Use an empty list if they did not say.\n"
    '- "depth": "quick", "standard" or "deep" only if they said how thorough to be, otherwise null.\n'
    '- "language": the language of the prompt: "te", "hi" or "en".\n'
    'Reply with JSON only: {"prompt": "", "reference_output": null, "target_hint": null, "audit_types": [], "depth": null, "language": "te"}'
)

TARGET_KEYWORDS = [
    (("gemma", "జెమ్మా", "జెమ్మ", "गेम्मा", "जेम्मा"), "gemma4"),
    (("deepseek", "deep seek", "డీప్‌సీక్", "డీప్ సీక్", "डीपसीक", "डीप सीक"), "deepseekv4.1-flash-1017"),
]

_QUOTES = [('"', '"'), ("“", "”"), ("‘", "’"), ("«", "»"), ("「", "」")]


def quoted_segments(text: str) -> list[str]:
    found: list[str] = []
    for opener, closer in _QUOTES:
        found += re.findall(re.escape(opener) + r"(.+?)" + re.escape(closer), text, flags=re.DOTALL)
    return [s.strip() for s in found if len(s.strip()) >= 4]


def resolve_target(hint: str | None, text: str) -> str | None:
    haystack = f"{hint or ''} {text}".lower()
    for keywords, model_id in TARGET_KEYWORDS:
        if any(k.lower() in haystack for k in keywords):
            return model_id
    return None


class Orchestrator:
    def __init__(self, chat: ChatClient, settings: Settings):
        self.chat = chat
        self.settings = settings

    async def plan(self, text: str, *, overrides: dict | None = None) -> Plan:
        text = text.strip()
        notes: list[str] = []
        data: dict | None = None
        try:
            data = await self._extract(text)
        except (AuthError, OutOfCredits):
            raise
        except Exception:
            notes.append("I couldn't use the language model to read your message, so I'm testing it as you wrote it.")

        prompt = None
        if data and isinstance(data.get("prompt"), str) and data["prompt"].strip():
            cand = data["prompt"].strip()
            if squash(cand) in squash(text):
                prompt = cand
            else:
                notes.append("The text I pulled out didn't match what you said, so I went back to your own words.")
        if not prompt:
            quotes = quoted_segments(text)
            prompt = max(quotes, key=len) if quotes else text
            if not quotes and data is not None and "went back to your own words" not in " ".join(notes):
                notes.append("I couldn't spot a separate question, so I'll test your whole message.")

        limit = self.settings.max_prompt_chars
        if len(prompt) > limit:
            prompt = prompt[:limit].rsplit(" ", 1)[0].strip()
            notes.append(f"That was long, so I kept the first {limit} characters.")

        reference = data.get("reference_output") if data else None
        reference = reference.strip() if isinstance(reference, str) and reference.strip() else None

        target = TargetSpec(model=self.settings.target_model)
        hinted = resolve_target(data.get("target_hint") if data else None, text)
        if hinted:
            target = TargetSpec(model=hinted)

        types = [t for t in (data.get("audit_types") or []) if t in ALL_TYPES] if data else []
        depth = data.get("depth") if data and data.get("depth") in ("quick", "standard", "deep") else "standard"
        language = dominant_language(prompt, default="en")
        if language not in ("te", "hi", "en"):
            language = "en"

        plan = Plan(
            prompt=prompt, reference_output=reference, target=target,
            audit_types=types or list(ALL_TYPES), depth=depth,
            prompt_language=language, notes=notes, source_text=text,
        )
        return self._apply(plan, overrides)

    @staticmethod
    def _apply(plan: Plan, overrides: dict | None) -> Plan:
        if not overrides:
            return plan
        merged = plan.model_dump()
        for key in ("depth", "audit_types", "target", "reference_output"):
            if overrides.get(key) not in (None, "", []):
                merged[key] = overrides[key]
        return Plan(**merged)

    async def _extract(self, text: str) -> dict | None:
        reply = await self.chat.complete(
            self.settings.plan_model,
            [{"role": "system", "content": SYSTEM}, {"role": "user", "content": text}],
            temperature=0.0, max_tokens=600, json_mode=True,
        )
        data = extract_json(reply.text)
        return data if isinstance(data, dict) else None
