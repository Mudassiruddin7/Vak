"""Plain facts about the languages, models, voices and prices Vak deals with.

Prices are Sarvam's pay-as-you-go list prices in rupees, read off
https://docs.sarvam.ai/api/getting-started/pricing on 4 Nov 2026. They only
drive the budget meter, so if they drift the worst case is a slightly wrong
rupee figure on screen.
"""
from __future__ import annotations

from dataclasses import dataclass


# --- languages ------------------------------------------------------------

@dataclass(frozen=True)
class Lang:
    code: str        # short code used everywhere inside Vak
    bcp47: str       # what Sarvam wants
    english: str
    native: str


LANGS: dict[str, Lang] = {
    "te": Lang("te", "te-IN", "Telugu", "తెలుగు"),
    "hi": Lang("hi", "hi-IN", "Hindi", "हिन्दी"),
    "en": Lang("en", "en-IN", "English", "English"),
}

EXPLAIN_LANGS = ("te", "hi")


def bcp47(code: str) -> str:
    return LANGS[code].bcp47


# --- chat models ----------------------------------------------------------

@dataclass(frozen=True)
class ChatModel:
    id: str
    label: str
    api: str                # "v1" or "v2" path prefix on api.sarvam.ai
    off_switch: dict        # extra body fields that keep it from "thinking"
    inr_in: float           # rupees per 1M input tokens
    inr_out: float          # rupees per 1M output tokens
    blurb: str


CHAT_MODELS: dict[str, ChatModel] = {
    m.id: m
    for m in (
        ChatModel(
            "sarvam-105b-conversations", "Sarvam 105B (conversational)", "v1", {},
            15.0, 60.0, "Tuned for everyday dialogue. The default target.",
        ),
        ChatModel(
            "sarvam-105b", "Sarvam 105B", "v1", {"reasoning_effort": None},
            15.0, 60.0, "The flagship, with its thinking mode switched off.",
        ),
        ChatModel(
            "gemma4", "Gemma 4 31B", "v2", {},
            35.0, 90.0, "Open-weight model served through Sarvam.",
        ),
        ChatModel(
            "deepseekv4.1-flash-1017", "DeepSeek V4.1 Flash", "v2", {"reasoning_effort": "none"},
            25.0, 105.0, "Open-weight model served through Sarvam.",
        ),
    )
}


def chat_api_for(model_id: str) -> str:
    info = CHAT_MODELS.get(model_id)
    if info:
        return info.api
    return "v1" if model_id.startswith("sarvam-105b") else "v2"


def chat_cost_inr(model_id: str, prompt_tokens: int, completion_tokens: int) -> float:
    info = CHAT_MODELS.get(model_id)
    if info is None:
        # Unknown model: charge at the flagship rate so the meter errs on the high side.
        info = CHAT_MODELS["sarvam-105b"]
    return (prompt_tokens * info.inr_in + completion_tokens * info.inr_out) / 1_000_000


# --- speech and text prices ----------------------------------------------

STT_INR_PER_MIN = 0.49
TTS_INR_PER_1K_CHARS = {"bulbul:v4-flash": 1.00, "bulbul:v3": 3.00}
TRANSLATE_INR_PER_1K_CHARS = 2.00
TRANSLITERATE_INR_PER_1K_CHARS = 2.00


def tts_cost_inr(model: str, chars: int) -> float:
    return TTS_INR_PER_1K_CHARS.get(model, 3.00) * chars / 1000


# --- voices ---------------------------------------------------------------

@dataclass(frozen=True)
class Voice:
    id: str
    name: str
    gender: str     # "F" or "M"


VOICES: dict[str, tuple[Voice, ...]] = {
    "te": (
        Voice("kavitha_te_conversation", "Kavitha", "F"),
        Voice("pooja_te_conversation", "Pooja", "F"),
        Voice("tarun_te_narration", "Tarun", "M"),
    ),
    "hi": (
        Voice("ritu_hi_conversation", "Ritu", "F"),
        Voice("roopa_hi_conversational", "Roopa", "F"),
        Voice("kabir_hi_conversational", "Kabir", "M"),
        Voice("rahul_hi_conversational", "Rahul", "M"),
    ),
}


# --- budget pots from the project brief ----------------------------------

DEFAULT_BUDGET_INR = {
    "stt": 5000.0,
    "tts": 4000.0,
    "llm": 10000.0,
    "translate": 3000.0,   # translation and transliteration share this pot
    "buffer": 3000.0,      # absorbs overspend from any other pot
}

# Audit depth presets. Counts are per-category upper bounds.
DEPTHS = {
    "quick":    dict(resamples=2, typo=2, format=1, paraphrase=2, language=1, distractor=1, polite=0, negation=1, control=1, units=6),
    "standard": dict(resamples=3, typo=3, format=2, paraphrase=3, language=2, distractor=2, polite=1, negation=1, control=1, units=10),
    "deep":     dict(resamples=5, typo=5, format=3, paraphrase=5, language=2, distractor=3, polite=2, negation=2, control=2, units=16),
}
