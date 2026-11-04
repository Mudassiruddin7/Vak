"""Small helpers shared across the package."""
from __future__ import annotations

import hashlib
import json
import re
from typing import Any

_THINK_BLOCK = re.compile(r"<think>.*?</think>", re.DOTALL | re.IGNORECASE)
_THINK_OPEN = re.compile(r"<think>.*\Z", re.DOTALL | re.IGNORECASE)
_FENCE = re.compile(r"```(?:json)?\s*(.*?)```", re.DOTALL | re.IGNORECASE)


def digest(*parts: Any) -> str:
    """Stable hash of anything JSON-serialisable. Used for cache keys and file names."""
    blob = json.dumps(parts, ensure_ascii=False, sort_keys=True, default=str)
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()


def strip_think(text: str) -> str:
    """Drop <think>...</think> blocks, including one that was cut off by a length limit."""
    if not text:
        return ""
    text = _THINK_BLOCK.sub("", text)
    text = _THINK_OPEN.sub("", text)
    return text.strip()


def extract_json(text: str) -> Any | None:
    """Pull the first JSON object or array out of model output, or None."""
    if not text:
        return None
    candidates = [text.strip()]
    candidates += [m.strip() for m in _FENCE.findall(text)]
    for cand in candidates:
        try:
            return json.loads(cand)
        except ValueError:
            pass
    for opener, closer in (("{", "}"), ("[", "]")):
        start = text.find(opener)
        while start != -1:
            depth, in_str, esc = 0, False, False
            for i in range(start, len(text)):
                ch = text[i]
                if in_str:
                    if esc:
                        esc = False
                    elif ch == "\\":
                        esc = True
                    elif ch == '"':
                        in_str = False
                    continue
                if ch == '"':
                    in_str = True
                elif ch == opener:
                    depth += 1
                elif ch == closer:
                    depth -= 1
                    if depth == 0:
                        try:
                            return json.loads(text[start:i + 1])
                        except ValueError:
                            break
            start = text.find(opener, start + 1)
    return None


def clip(text: str, limit: int) -> str:
    text = text.strip()
    return text if len(text) <= limit else text[: limit - 1].rstrip() + "…"
