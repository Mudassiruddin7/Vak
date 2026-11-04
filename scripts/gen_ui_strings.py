# /// script
# requires-python = ">=3.11"
# dependencies = ["httpx", "python-dotenv"]
# ///
"""Machine-translate the interface strings into the other Indian languages with Sarvam-Translate.

    scripts/export_ui_strings.sh > /tmp/ui-en.json
    uv run scripts/gen_ui_strings.py /tmp/ui-en.json            # every language
    uv run scripts/gen_ui_strings.py /tmp/ui-en.json ta kn      # just these

Writes web/i18n/<code>.json and web/i18n/index.json. Placeholders such as {n} and the <em> tags are
protected, and any string whose placeholders did not come back intact is left out, so the page falls back to
English for it. Everything written is machine-translated, and the page says so. Costs about Rs 10 a language.
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
OUT = ROOT / "web" / "i18n"
CODES = ["as", "bn", "brx", "doi", "gu", "kn", "kok", "ks", "mai", "ml", "mni", "mr", "ne", "od", "pa", "sa", "sat", "sd", "ta", "ur"]
BCP = {c: f"{c}-IN" for c in CODES}
PROTECT = re.compile(r"\{[a-zA-Z_]+\}|</?em>|<br>")


class Client:
    def __init__(self, key: str):
        self.http = httpx.AsyncClient(base_url="https://api.sarvam.ai", headers={"api-subscription-key": key}, timeout=60)
        self.last = 0.0
        self.lock = asyncio.Lock()
        self.chars = 0

    async def translate(self, text: str, code: str) -> str:
        self.chars += len(text)
        for attempt in range(5):
            async with self.lock:
                wait = self.last + 60 / 44 - asyncio.get_event_loop().time()
                if wait > 0:
                    await asyncio.sleep(wait)
                self.last = asyncio.get_event_loop().time()
            r = await self.http.post("/translate", json={"input": text, "source_language_code": "en-IN", "target_language_code": BCP[code],
                                                         "model": "sarvam-translate:v1", "numerals_format": "international"})
            if r.status_code in (429, 500, 502, 503):
                await asyncio.sleep(2 ** attempt)
                continue
            r.raise_for_status()
            return r.json()["translated_text"].strip()
        raise RuntimeError("translate kept failing")


def shield(text: str) -> tuple[str, list[str]]:
    found: list[str] = []

    def sub(m: re.Match) -> str:
        found.append(m.group(0))
        return f"[[{len(found)}]]"

    return PROTECT.sub(sub, text), found


def unshield(text: str, found: list[str]) -> str | None:
    for i, original in enumerate(found, 1):
        if f"[[{i}]]" not in text:
            return None
        text = text.replace(f"[[{i}]]", original)
    return None if "[[" in text else text


async def one_language(c: Client, code: str, strings: dict[str, str]) -> dict[str, str]:
    keys = list(strings)
    shielded = {k: shield(strings[k]) for k in keys}
    out: dict[str, str] = {}
    for i in range(0, len(keys), 12):
        batch = keys[i:i + 12]
        got = (await c.translate("\n".join(shielded[k][0] for k in batch), code)).splitlines()
        got = [g.strip() for g in got if g.strip()]
        if len(got) != len(batch):
            got = [await c.translate(shielded[k][0], code) for k in batch]
        for k, g in zip(batch, got):
            fixed = unshield(g, shielded[k][1])
            if fixed:
                out[k] = fixed
    return out


async def main(src: Path, codes: list[str]) -> None:
    key = dotenv_values(ROOT / ".env").get("SARVAM_API_KEY") or ""
    if not key:
        sys.exit("SARVAM_API_KEY missing from .env")
    strings = json.loads(src.read_text(encoding="utf-8"))
    OUT.mkdir(parents=True, exist_ok=True)
    c = Client(key)
    done = [p.stem for p in OUT.glob("*.json") if p.stem != "index"]
    try:
        for code in codes:
            path = OUT / f"{code}.json"
            have = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
            # incremental: only strings that are not translated yet, so a re-run after adding screens is cheap
            todo = {k: v for k, v in strings.items() if k not in have}
            table = {**have, **(await one_language(c, code, todo) if todo else {})}
            path.write_text(json.dumps(table, ensure_ascii=False, indent=0), encoding="utf-8")
            done.append(code)
            print(f"{code}: {len(table)}/{len(strings)} strings ({len(todo)} new), {c.chars} chars so far (about Rs {c.chars / 1000 * 2:.0f})", flush=True)
    finally:
        await c.http.aclose()
        (OUT / "index.json").write_text(json.dumps(sorted(set(done))), encoding="utf-8")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    asyncio.run(main(Path(sys.argv[1]), sys.argv[2:] or CODES))
