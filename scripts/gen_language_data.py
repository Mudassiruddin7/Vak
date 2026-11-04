# /// script
# requires-python = ">=3.11"
# dependencies = ["httpx", "python-dotenv"]
# ///
"""Generate the machine-translated language data for the languages that have no hand-written wording.

    uv run scripts/gen_language_data.py ta kn ml mr bn gu pa od

Reads crates/vak-core/data/en_sources.json, asks Sarvam-Translate for each line (and /transliterate for the
banned jargon), and writes crates/vak-core/data/generated/<code>.json. Everything it writes is flagged
`machine_translated`, because no native speaker has reviewed it. Costs roughly Rs 8 per language.
"""
from __future__ import annotations

import asyncio
import json
import re
import sys
from pathlib import Path

import httpx
from dotenv import dotenv_values

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/vak-core/data/en_sources.json"
OUT = ROOT / "crates/vak-core/data/generated"
BCP47 = {"te": "te-IN", "hi": "hi-IN", "ta": "ta-IN", "kn": "kn-IN", "ml": "ml-IN", "mr": "mr-IN",
         "bn": "bn-IN", "gu": "gu-IN", "pa": "pa-IN", "od": "od-IN",
         "as": "as-IN", "brx": "brx-IN", "doi": "doi-IN", "kok": "kok-IN", "ks": "ks-IN", "mai": "mai-IN",
         "mni": "mni-IN", "ne": "ne-IN", "sa": "sa-IN", "sat": "sat-IN", "sd": "sd-IN", "ur": "ur-IN"}
BASE = "https://api.sarvam.ai"
RPM = 45


class Client:
    def __init__(self, key: str):
        self.http = httpx.AsyncClient(base_url=BASE, headers={"api-subscription-key": key}, timeout=60)
        self.gap = 60 / RPM
        self.lock = asyncio.Lock()
        self.last = 0.0
        self.spent_chars = 0

    async def _post(self, path: str, body: dict) -> dict:
        for attempt in range(5):
            async with self.lock:
                wait = self.last + self.gap - asyncio.get_event_loop().time()
                if wait > 0:
                    await asyncio.sleep(wait)
                self.last = asyncio.get_event_loop().time()
            r = await self.http.post(path, json=body)
            if r.status_code in (429, 500, 502, 503):
                await asyncio.sleep(2 ** attempt)
                continue
            r.raise_for_status()
            return r.json()
        raise RuntimeError(f"{path} kept failing")

    async def translate(self, text: str, dst: str) -> str:
        self.spent_chars += len(text)
        j = await self._post("/translate", {"input": text, "source_language_code": "en-IN", "target_language_code": BCP47[dst],
                                            "model": "sarvam-translate:v1", "numerals_format": "international"})
        return j["translated_text"].strip()

    async def translit(self, text: str, dst: str) -> str:
        """Empty when the transliterator doesn't cover this language; the English words are banned anyway."""
        try:
            j = await self._post("/transliterate", {"input": text, "source_language_code": "en-IN", "target_language_code": BCP47[dst]})
            return j["transliterated_text"].strip()
        except Exception:
            return ""


async def lines(c: Client, items: list[str], dst: str) -> list[str]:
    """Translate several lines in one call when the line count survives, otherwise one by one."""
    out: list[str] = []
    for i in range(0, len(items), 10):
        batch = items[i:i + 10]
        joined = "\n".join(batch)
        got = (await c.translate(joined, dst)).splitlines()
        got = [g.strip() for g in got if g.strip()]
        if len(got) != len(batch):
            got = [await c.translate(b, dst) for b in batch]
        out += got
    return out


def with_placeholder(text: str, token: str, sample: str) -> str:
    return text.replace(token, sample)


async def build(c: Client, code: str, src: dict) -> dict:
    bank = src["bank"]
    flat: list[tuple[str, str | None, str]] = []   # (group, key, english)
    for k, v in bank.items():
        if isinstance(v, str):
            flat.append((k, None, v))
        elif isinstance(v, list):
            flat += [(k, str(i), s) for i, s in enumerate(v)]
        else:
            flat += [(k, kk, s) for kk, s in v.items()]
    # keep {n} and {w} alive by translating a concrete stand-in and swapping it back
    prepared = [s.replace("{n}", "77").replace("{w}", "ZYX") for _, _, s in flat]
    done = await lines(c, prepared, code)
    result: dict = {}
    problems = []
    for (group, key, eng), got in zip(flat, done):
        if "{n}" in eng:
            if "77" not in got:
                problems.append(f"{group}.{key}: lost the number")
            got = got.replace("77", "{n}")
        if "{w}" in eng:
            if "ZYX" not in got:
                problems.append(f"{group}.{key}: lost the word slot")
            got = got.replace("ZYX", "{w}")
        if key is None:
            result[group] = got
        elif group in ("concern", "mixed", "steady"):
            result.setdefault(group, []).append(got)
        else:
            result.setdefault(group, {})[key] = got

    v = src["variants"]
    variants = {
        "distractors": await lines(c, v["distractors"], code),
        "controls": await lines(c, v["controls"], code),
        "polite": [{"prefix": (await c.translate(v["polite_prefix"], code)) + " ", "suffix": ""},
                   {"prefix": "", "suffix": " " + await c.translate(v["polite_suffix"], code)}],
    }
    notices = dict(zip(src["notices"], await lines(c, list(src["notices"].values()), code)))
    jargon = []
    for term in src["jargon"]:
        got = await c.translit(term, code)
        if got:
            jargon.append(got)
    return {"code": code, "machine_translated": True, "model": "sarvam-translate:v1", "bank": result,
            "variants": variants, "notices": notices, "jargon": jargon, "problems": problems}


async def main(codes: list[str]) -> None:
    key = dotenv_values(ROOT / ".env").get("SARVAM_API_KEY") or ""
    if not key:
        sys.exit("SARVAM_API_KEY missing from .env")
    src = json.loads(SRC.read_text(encoding="utf-8"))
    OUT.mkdir(parents=True, exist_ok=True)
    c = Client(key)
    try:
        for code in codes:
            data = await build(c, code, src)
            (OUT / f"{code}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")
            print(f"{code}: wrote {len(json.dumps(data))} bytes, {len(data['problems'])} placeholder problems, "
                  f"{c.spent_chars} chars translated so far (about Rs {c.spent_chars / 1000 * 2:.1f})")
            for p in data["problems"]:
                print("   !", p)
    finally:
        await c.http.aclose()


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1:] or ["ta", "kn", "ml", "mr", "bn", "gu", "pa", "od"]))
