"""Deciding whether two answers say the same thing.

Two layers. A cheap lexical check settles the obvious cases (identical text, or
near-identical with the same numbers). Everything else goes to a language-model
judge in batches. If the judge is unavailable the lexical score still gives a
verdict, so an audit never stalls on this step.
"""
from __future__ import annotations

import asyncio
import math
from collections import Counter
from dataclasses import dataclass

from ..sarvam import AuthError, ChatClient, OutOfCredits
from ..util import clip, extract_json
from .text_utils import numbers_in, squash

NEAR_IDENTICAL = 0.92
SCORES = {"same": 1.0, "partly": 0.5, "different": 0.0}


def _ngrams(text: str, n: int = 3) -> Counter:
    padded = f" {text} "
    return Counter(padded[i:i + n] for i in range(max(1, len(padded) - n + 1)))


def _cosine(a: Counter, b: Counter) -> float:
    dot = sum(v * b.get(k, 0) for k, v in a.items())
    na = math.sqrt(sum(v * v for v in a.values()))
    nb = math.sqrt(sum(v * v for v in b.values()))
    return dot / (na * nb) if na and nb else 0.0


def lexical_similarity(a: str, b: str) -> float:
    """0..1 from character trigrams and word overlap, after forgiving case, punctuation and vowel length."""
    na, nb = squash(a), squash(b)
    if not na and not nb:
        return 1.0
    if not na or not nb:
        return 0.0
    if na == nb:
        return 1.0
    cos = _cosine(_ngrams(na), _ngrams(nb))
    wa, wb = Counter(na.split()), Counter(nb.split())
    overlap = sum((wa & wb).values())
    f1 = 2 * overlap / (sum(wa.values()) + sum(wb.values()))
    return 0.5 * cos + 0.5 * f1


def settled_without_judge(a: str, b: str) -> bool:
    """True when the answers are effectively the same text, numbers included."""
    if squash(a) == squash(b):
        return True
    return lexical_similarity(a, b) >= NEAR_IDENTICAL and numbers_in(a) == numbers_in(b)


def lexical_verdict(a: str, b: str) -> tuple[str, float]:
    if settled_without_judge(a, b):
        return "same", 1.0
    lex = lexical_similarity(a, b)
    if lex >= 0.7 and numbers_in(a) == numbers_in(b):
        return "same", lex
    if lex >= 0.4:
        return "partly", lex
    return "different", lex


@dataclass
class Verdict:
    verdict: str          # same | partly | different
    why: str = ""


_JUDGE_SYSTEM = (
    "You are a strict but fair grader comparing answers from an AI model.\n"
    "You get a reference answer and numbered candidate answers. The candidates came from lightly edited versions "
    "of the same task, so judge the answers only.\n"
    "For each candidate decide:\n"
    '- "same": the same final answer, decision or meaning as the reference. Wording, language, politeness and length do not matter.\n'
    '- "partly": mostly the same, but a material detail differs: a number, a condition, a step or a caveat.\n'
    '- "different": a different final answer, a contradiction, a refusal where the other answered, or unrelated content.\n'
    "Candidates may be in another language than the reference. Compare meaning, not wording.\n"
    'Reply with JSON only: {"verdicts": [{"i": 1, "v": "same", "why": "at most 12 words"}]}'
)


class Judge:
    def __init__(self, chat: ChatClient, model: str, batch_size: int = 8):
        self.chat = chat
        self.model = model
        self.batch_size = batch_size

    async def compare(self, task: str, reference: str, candidates: dict[str, str]) -> dict[str, Verdict]:
        """Verdicts for as many candidates as the judge answered. Missing ids simply aren't in the result."""
        ids = list(candidates)
        batches = [ids[i:i + self.batch_size] for i in range(0, len(ids), self.batch_size)]
        results = await asyncio.gather(
            *(self._batch(task, reference, {k: candidates[k] for k in batch}) for batch in batches),
            return_exceptions=True,
        )
        merged: dict[str, Verdict] = {}
        for res in results:
            if isinstance(res, (AuthError, OutOfCredits)):
                raise res
            if isinstance(res, dict):
                merged.update(res)
        return merged

    async def _batch(self, task: str, reference: str, candidates: dict[str, str]) -> dict[str, Verdict]:
        keys = list(candidates)
        lines = "\n".join(f"{i}. <<<{clip(candidates[k], 600)}>>>" for i, k in enumerate(keys, 1))
        user = f"Task: <<<{clip(task, 500)}>>>\nReference answer: <<<{clip(reference, 700)}>>>\n\nCandidates:\n{lines}"
        reply = await self.chat.complete(
            self.model,
            [{"role": "system", "content": _JUDGE_SYSTEM}, {"role": "user", "content": user}],
            temperature=0.0, max_tokens=90 * len(keys) + 150, json_mode=True,
        )
        data = extract_json(reply.text)
        rows = data.get("verdicts") if isinstance(data, dict) else data if isinstance(data, list) else None
        out: dict[str, Verdict] = {}
        for row in rows or []:
            if not isinstance(row, dict):
                continue
            try:
                key = keys[int(row.get("i")) - 1]
            except (TypeError, ValueError, IndexError):
                continue
            v = str(row.get("v", "")).strip().lower()
            if v in SCORES:
                out[key] = Verdict(v, str(row.get("why", "")).strip()[:140])
        return out
