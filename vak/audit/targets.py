"""The model being audited. Sarvam-hosted, your own OpenAI-compatible endpoint, or a built-in demo."""
from __future__ import annotations

import ipaddress
import socket
from dataclasses import dataclass
from urllib.parse import urlparse

import httpx

from ..catalog import CHAT_MODELS
from ..sarvam import ChatClient
from ..util import strip_think
from .models import TargetSpec


class TargetError(Exception):
    """The audited model couldn't be reached or answered badly. Fine for one test, fatal for the baseline."""


@dataclass
class Answer:
    text: str
    error: str | None = None


class Target:
    label: str = "model"
    billed: bool = True      # whether calls spend Sarvam credits
    can_explain: bool = True  # False for rule-based stand-ins that can't describe their own reasons

    async def ask(self, prompt: str, *, temperature: float = 0.0, n: int = 1, max_tokens: int = 420) -> list[Answer]:
        raise NotImplementedError


def _messages(prompt: str, system: str | None) -> list[dict]:
    msgs = [{"role": "system", "content": system}] if system else []
    msgs.append({"role": "user", "content": prompt})
    return msgs


class SarvamTarget(Target):
    def __init__(self, chat: ChatClient, model: str, system_prompt: str | None, use_cache: bool = True):
        self.chat, self.model, self.system, self.use_cache = chat, model, system_prompt, use_cache
        info = CHAT_MODELS.get(model)
        self.label = info.label if info else model

    async def ask(self, prompt, *, temperature=0.0, n=1, max_tokens=420):
        reply = await self.chat.complete(
            self.model, _messages(prompt, self.system),
            temperature=temperature, max_tokens=max_tokens, n=n,
            use_cache=self.use_cache and temperature == 0,
        )
        answers = [Answer(t) if t else Answer("", error="The model sent back an empty answer.") for t in reply.texts]
        while len(answers) < n:
            answers.append(Answer("", error="The model returned fewer answers than asked for."))
        return answers


def check_endpoint(url: str, *, allow_private: bool) -> str:
    """Validate a user-supplied endpoint. Cloud metadata addresses are never allowed."""
    parts = urlparse(url)
    if parts.scheme not in ("http", "https") or not parts.hostname:
        raise TargetError("The endpoint must be an http(s) URL, for example http://localhost:11434/v1")
    try:
        infos = socket.getaddrinfo(parts.hostname, parts.port or (443 if parts.scheme == "https" else 80))
    except socket.gaierror as exc:
        raise TargetError(f"Couldn't find the host {parts.hostname!r}.") from exc
    for info in infos:
        ip = ipaddress.ip_address(info[4][0])
        if ip.is_link_local or ip.is_multicast or ip.is_unspecified:
            raise TargetError("That address isn't allowed.")
        if (ip.is_private or ip.is_loopback) and not allow_private:
            raise TargetError("Private addresses are switched off on this server.")
    return url.rstrip("/")


class CustomTarget(Target):
    billed = False

    def __init__(self, base_url: str, api_key: str | None, model: str, system_prompt: str | None, *, allow_private: bool):
        self.base = check_endpoint(base_url, allow_private=allow_private)
        self.key, self.model, self.system = api_key, model, system_prompt
        self.label = f"{model} (your endpoint)"

    async def ask(self, prompt, *, temperature=0.0, n=1, max_tokens=420):
        body = {"model": self.model, "messages": _messages(prompt, self.system), "temperature": temperature, "max_tokens": max_tokens}
        headers = {"Authorization": f"Bearer {self.key}"} if self.key else {}
        answers: list[Answer] = []
        async with httpx.AsyncClient(timeout=httpx.Timeout(60.0, connect=8.0)) as client:
            for _ in range(n):   # plain OpenAI-compatible servers don't all support n>1
                try:
                    resp = await client.post(f"{self.base}/chat/completions", json=body, headers=headers)
                    resp.raise_for_status()
                    text = strip_think(resp.json()["choices"][0]["message"].get("content") or "")
                    answers.append(Answer(text) if text else Answer("", error="Empty answer."))
                except (httpx.HTTPError, KeyError, IndexError, ValueError) as exc:
                    raise TargetError(f"Your endpoint didn't answer properly ({type(exc).__name__}).") from exc
        return answers


def build_target(spec: TargetSpec, *, chat: ChatClient, allow_custom: bool, allow_private: bool, fresh: bool = False) -> Target:
    if spec.kind == "demo":
        from .demo import build_demo
        return build_demo(spec.model)
    if spec.kind == "custom":
        if not allow_custom:
            raise TargetError("Auditing your own endpoint is switched off on this server.")
        if not (spec.base_url and spec.remote_model):
            raise TargetError("Give the endpoint URL and the model name to audit your own model.")
        return CustomTarget(spec.base_url, spec.api_key, spec.remote_model, spec.system_prompt, allow_private=allow_private)
    return SarvamTarget(chat, spec.model, spec.system_prompt, use_cache=not fresh)
