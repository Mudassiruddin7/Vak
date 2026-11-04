"""Translation (Mayura) and transliteration."""
from __future__ import annotations

from ..audio_util import split_for_tts
from ..budget import Budget
from ..catalog import TRANSLATE_INR_PER_1K_CHARS, TRANSLITERATE_INR_PER_1K_CHARS, bcp47
from ..config import Settings
from ..store import Store
from ..util import digest
from .http import SarvamHTTP, Unavailable

TRANSLATE_CHUNK = 900   # Mayura accepts 1,000 characters per call


class TextClient:
    def __init__(self, http: SarvamHTTP, settings: Settings, store: Store, budget: Budget):
        self.http = http
        self.settings = settings
        self.store = store
        self.budget = budget

    async def translate(self, text: str, src: str, dst: str, *, mode: str = "formal") -> str:
        text = text.strip()
        if not text or src == dst:
            return text
        pieces = split_for_tts(text, TRANSLATE_CHUNK)
        out = [await self._translate_piece(p, src, dst, mode) for p in pieces]
        return " ".join(out).strip()

    async def _translate_piece(self, text: str, src: str, dst: str, mode: str) -> str:
        key = digest("translate", self.settings.translate_model, src, dst, mode, text)
        hit = self.store.cache_get(key)
        if hit is not None:
            return hit
        body = {
            "input": text,
            "source_language_code": bcp47(src),
            "target_language_code": bcp47(dst),
            "model": self.settings.translate_model,
            "mode": mode,
        }
        resp = await self.http.request("text", "POST", "/translate", json=body)
        translated = (resp.json().get("translated_text") or "").strip()
        if not translated:
            raise Unavailable("The translator sent back nothing.")
        self.budget.record("translate", len(text), "chars", len(text) / 1000 * TRANSLATE_INR_PER_1K_CHARS, note=f"{src}->{dst}")
        self.store.cache_put(key, translated)
        return translated

    async def transliterate(self, text: str, src: str, dst: str) -> str:
        """Same words, different script, e.g. Telugu letters to the Roman letters people type on phones."""
        text = text.strip()
        if not text:
            return text
        key = digest("transliterate", src, dst, text)
        hit = self.store.cache_get(key)
        if hit is not None:
            return hit
        body = {
            "input": text,
            "source_language_code": bcp47(src),
            "target_language_code": bcp47(dst),
        }
        resp = await self.http.request("text", "POST", "/transliterate", json=body)
        result = (resp.json().get("transliterated_text") or "").strip()
        if not result:
            raise Unavailable("The transliterator sent back nothing.")
        self.budget.record("translate", len(text), "chars", len(text) / 1000 * TRANSLITERATE_INR_PER_1K_CHARS, note=f"{src}->{dst} script")
        self.store.cache_put(key, result)
        return result
