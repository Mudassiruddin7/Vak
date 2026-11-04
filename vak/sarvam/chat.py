"""Chat completions through Sarvam's /v1 and /v2 endpoints."""
from __future__ import annotations

import json
from dataclasses import asdict, dataclass

from ..budget import Budget
from ..catalog import CHAT_MODELS, chat_api_for, chat_cost_inr
from ..store import Store
from ..util import digest, strip_think
from .http import SarvamHTTP, Unavailable

MAX_TOKENS_CEILING = 4096   # what pay-as-you-go accounts are allowed to ask for


@dataclass
class Completion:
    texts: list[str]
    finish: list[str]
    prompt_tokens: int
    completion_tokens: int
    model: str
    cached: bool = False

    @property
    def text(self) -> str:
        return self.texts[0] if self.texts else ""


class ChatClient:
    def __init__(self, http: SarvamHTTP, store: Store, budget: Budget):
        self.http = http
        self.store = store
        self.budget = budget

    async def complete(
        self,
        model: str,
        messages: list[dict],
        *,
        temperature: float = 0.0,
        max_tokens: int = 512,
        n: int = 1,
        json_mode: bool = False,
        use_cache: bool | None = None,
    ) -> Completion:
        """One chat call. Deterministic calls (temperature 0) are cached on disk by default."""
        api = chat_api_for(model)
        body: dict = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": min(int(max_tokens), MAX_TOKENS_CEILING),
        }
        if n > 1:
            body["n"] = n
        if json_mode:
            body["response_format"] = {"type": "json_object"}
        info = CHAT_MODELS.get(model)
        if info:
            body.update(info.off_switch)

        cacheable = (temperature == 0) if use_cache is None else use_cache
        key = digest("chat", api, body)
        if cacheable:
            hit = self.store.cache_get(key)
            if hit:
                data = json.loads(hit)
                data["cached"] = True
                return Completion(**data)

        resp = await self.http.request("llm", "POST", f"/{api}/chat/completions", json=body)
        data = resp.json()
        choices = data.get("choices") or []
        if not choices:
            raise Unavailable("The model sent back an empty reply.")
        texts = [strip_think((c.get("message") or {}).get("content") or "") for c in choices]
        finish = [c.get("finish_reason") or "" for c in choices]
        usage = data.get("usage") or {}
        prompt_tokens = int(usage.get("prompt_tokens") or 0)
        completion_tokens = int(usage.get("completion_tokens") or 0)
        self.budget.record(
            "llm",
            prompt_tokens + completion_tokens,
            "tokens",
            chat_cost_inr(model, prompt_tokens, completion_tokens),
            note=model,
        )
        result = Completion(texts, finish, prompt_tokens, completion_tokens, model)
        if cacheable and any(texts):
            stored = asdict(result)
            stored.pop("cached")
            self.store.cache_put(key, json.dumps(stored, ensure_ascii=False))
        return result
