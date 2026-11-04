"""Which words of the prompt does the answer actually lean on?

Occlusion: take each word (or a short run of words, for long prompts) out, ask
again, and see how far the answer moves. A word whose removal changes nothing
did not matter to this answer; one whose removal flips it did.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

from .models import Unit
from .text_utils import tidy_spaces, trim_punct, word_spans

MIN_WORDS = 3          # shorter prompts have nothing meaningful to take apart
FLAT_FLOOR = 0.05      # below this, nothing moved the answer at all


@dataclass(frozen=True)
class UnitSpec:
    index: int
    text: str
    start: int
    end: int


def make_units(prompt: str, max_units: int) -> list[UnitSpec]:
    spans = [s for s in word_spans(prompt) if trim_punct(s.text)]
    if len(spans) < MIN_WORDS or max_units <= 0:
        return []
    size = math.ceil(len(spans) / max_units)
    groups = [spans[i:i + size] for i in range(0, len(spans), size)]
    return [UnitSpec(i, prompt[g[0].start:g[-1].end], g[0].start, g[-1].end) for i, g in enumerate(groups)]


def without_unit(prompt: str, unit: UnitSpec) -> str:
    return tidy_spaces(prompt[: unit.start] + " " + prompt[unit.end:])


def rank_units(units: list[UnitSpec], raw_by_index: dict[int, float | None]) -> list[Unit]:
    """Turn 'how far the answer moved when this word was removed' into importance (0..1, relative to the strongest word)."""
    raws = [max(0.0, min(1.0, raw_by_index[u.index])) if raw_by_index.get(u.index) is not None else 0.0 for u in units]
    peak = max(raws, default=0.0)
    out = []
    for u, raw in zip(units, raws):
        out.append(Unit(
            index=u.index, text=u.text, start=u.start, end=u.end,
            raw=round(raw, 3),
            importance=round(raw / peak, 3) if peak > FLAT_FLOOR else 0.0,
            changed=raw >= 0.4,
        ))
    return out
