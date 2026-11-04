"""Rupee budget: a ledger of what each call cost, per-pot balances, and audit cost estimates.

The pots mirror the project brief (speech-to-text, text-to-speech, language model,
translation, plus a buffer). When a pot runs dry the buffer absorbs the overspend.
"""
from __future__ import annotations

import math
import time
from collections.abc import Iterator
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass

from .catalog import (
    DEPTHS,
    STT_INR_PER_MIN,
    TRANSLATE_INR_PER_1K_CHARS,
    TRANSLITERATE_INR_PER_1K_CHARS,
    chat_cost_inr,
    tts_cost_inr,
)
from .store import Store

SERVICES = ("stt", "tts", "llm", "translate")
LABELS = {
    "stt": "Listening (speech to text)",
    "tts": "Speaking (text to speech)",
    "llm": "Thinking (language models)",
    "translate": "Translating",
}


class BudgetExhausted(Exception):
    pass


class Tally:
    """Rupees spent inside one `with budget.track()` block, across every task started in it."""

    def __init__(self) -> None:
        self.inr = 0.0


_current_tally: ContextVar[Tally | None] = ContextVar("vak_tally", default=None)


class Budget:
    def __init__(self, store: Store, allocations: dict[str, float]):
        self.store = store
        self.allocations = allocations

    @contextmanager
    def track(self) -> Iterator[Tally]:
        tally = Tally()
        token = _current_tally.set(tally)
        try:
            yield tally
        finally:
            _current_tally.reset(token)

    def record(self, service: str, units: float, unit: str, cost: float, note: str = "") -> None:
        if service not in SERVICES:
            raise ValueError(f"unknown service {service!r}")
        self.store.ledger_add(service, units, unit, cost, note)
        tally = _current_tally.get()
        if tally is not None:
            tally.inr += cost

    def summary(self) -> dict:
        totals = self.store.ledger_totals()
        pots, overflow, spent_all = {}, 0.0, 0.0
        for name in SERVICES:
            row = totals.get(name, {})
            spent = float(row.get("cost") or 0.0)
            allocated = float(self.allocations.get(name, 0.0))
            overflow += max(0.0, spent - allocated)
            spent_all += spent
            pots[name] = {
                "label": LABELS[name],
                "allocated": allocated,
                "spent": round(spent, 4),
                "remaining": round(max(0.0, allocated - spent), 4),
                "calls": int(row.get("calls") or 0),
                "units": float(row.get("units") or 0.0),
                "unit": row.get("unit") or "",
            }
        buffer_total = float(self.allocations.get("buffer", 0.0))
        total_allocated = sum(self.allocations.values())
        return {
            "pots": pots,
            "buffer": {
                "allocated": buffer_total,
                "spent": round(overflow, 4),
                "remaining": round(max(0.0, buffer_total - overflow), 4),
            },
            "total": {
                "allocated": total_allocated,
                "spent": round(spent_all, 4),
                "remaining": round(max(0.0, total_allocated - spent_all), 4),
            },
            "runway_days": self._runway_days(total_allocated - spent_all),
        }

    def _runway_days(self, remaining: float) -> float | None:
        """Days the remaining money lasts at the recent daily burn, or None with no history."""
        first = self.store.ledger_first_ts()
        if first is None:
            return None
        window_days = min(14.0, max(1.0, (time.time() - first) / 86400))
        burned = self.store.ledger_cost_since(time.time() - window_days * 86400)
        if burned <= 0:
            return None
        return round(remaining / (burned / window_days), 1)

    def is_dry(self, service: str) -> bool:
        s = self.summary()
        return s["pots"][service]["remaining"] <= 0 and s["buffer"]["remaining"] <= 0

    def ensure(self, needs: dict[str, float]) -> None:
        """Raise BudgetExhausted if a pot and the buffer together can't cover the estimate."""
        s = self.summary()
        spare = s["buffer"]["remaining"]
        for name, cost in needs.items():
            short = cost - s["pots"][name]["remaining"]
            if short > 0:
                if short > spare:
                    raise BudgetExhausted(
                        f"The {LABELS[name].lower()} pot is out of rupees and the buffer can't cover this run."
                    )
                spare -= short


# --- estimates ------------------------------------------------------------

@dataclass
class Estimate:
    calls: int
    llm_inr: float
    translate_inr: float
    tts_inr: float
    stt_inr: float

    @property
    def total_inr(self) -> float:
        return round(self.llm_inr + self.translate_inr + self.tts_inr + self.stt_inr, 2)

    def as_dict(self) -> dict:
        return {
            "calls": self.calls,
            "llm_inr": round(self.llm_inr, 2),
            "translate_inr": round(self.translate_inr, 2),
            "tts_inr": round(self.tts_inr, 2),
            "stt_inr": round(self.stt_inr, 2),
            "total_inr": self.total_inr,
        }


def estimate_audit(
    depth: str,
    *,
    prompt_chars: int,
    target_model: str,
    helper_model: str,
    target_is_billed: bool = True,
    voice: bool = False,
    voice_seconds: float = 0.0,
    explanation_chars: int = 900,
) -> Estimate:
    """Rough rupee estimate for one audit. Meant for the confirm screen and the cost cap."""
    preset = DEPTHS[depth]
    variants = sum(preset[k] for k in ("typo", "format", "paraphrase", "language", "distractor", "polite", "negation", "control"))
    variants += 1  # romanised-script variant
    target_calls = 2 + variants + preset["units"] + 1  # baseline, resamples, variants, word removals, self-explanation probe
    helper_calls = 3 + math.ceil((variants + preset["units"]) / 10)  # paraphrases, opposite, judge batches, plan
    explain_calls = 1.6

    def tokens_in(extra: int = 0) -> int:
        return max(30, int(prompt_chars * 0.55)) + extra

    target_in, target_out = tokens_in(15), 140
    target_inr = 0.0
    if target_is_billed:
        target_inr = target_calls * chat_cost_inr(target_model, target_in, target_out)
    helper_inr = helper_calls * chat_cost_inr(helper_model, 520, 180)
    explain_inr = explain_calls * chat_cost_inr(helper_model, 900, 520)

    translate_chars = preset["language"] * prompt_chars
    translate_inr = translate_chars / 1000 * TRANSLATE_INR_PER_1K_CHARS
    translate_inr += prompt_chars / 1000 * TRANSLITERATE_INR_PER_1K_CHARS

    return Estimate(
        calls=int(target_calls + helper_calls + explain_calls) + preset["language"] + 1,
        llm_inr=target_inr + helper_inr + explain_inr,
        translate_inr=translate_inr,
        tts_inr=tts_cost_inr("bulbul:v4-flash", explanation_chars) if voice else 0.0,
        stt_inr=voice_seconds / 60 * STT_INR_PER_MIN,
    )
