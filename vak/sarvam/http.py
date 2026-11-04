"""Async HTTP wrapper for api.sarvam.ai: auth header, per-service throttling,
retries with backoff, and error mapping.

Retry rules follow Sarvam's own guidance: retry 429, 500, 502, 503 and 504,
never 400, 402, 403 or 422, and don't keep hammering when the spend budget is gone.
"""
from __future__ import annotations

import asyncio
import random
import time
from typing import Any

import httpx

from ..config import Settings

RETRY_STATUS = {429, 500, 502, 503, 504}
NO_RETRY_CODES = {"seat_budget_exhausted_error", "insufficient_quota_error"}


class SarvamError(Exception):
    def __init__(self, message: str, *, status: int | None = None, code: str | None = None, request_id: str | None = None):
        super().__init__(message)
        self.message = message
        self.status = status
        self.code = code
        self.request_id = request_id


class AuthError(SarvamError):
    """Missing or rejected API key (Sarvam answers 403, not 401)."""


class OutOfCredits(SarvamError):
    """The account's credit balance is empty. Retrying won't help."""


class RateLimited(SarvamError):
    pass


class BadRequest(SarvamError):
    pass


class Unavailable(SarvamError):
    pass


def _error_from(resp: httpx.Response) -> SarvamError:
    message, code, request_id = "", None, None
    try:
        body = resp.json()
    except ValueError:
        body = None
    if isinstance(body, dict):
        err = body.get("error")
        if isinstance(err, dict):
            message = str(err.get("message") or "")
            code = err.get("code")
            request_id = err.get("request_id")
        elif isinstance(body.get("detail"), str):
            message = body["detail"]
    if not message:
        message = (resp.text or "").strip()[:200] or f"HTTP {resp.status_code}"
    status = resp.status_code
    if status == 402:
        cls = OutOfCredits
    elif status in (401, 403):
        cls = AuthError
    elif status == 429:
        cls = RateLimited
    elif status in (500, 502, 503, 504):
        cls = Unavailable
    else:
        cls = BadRequest
    return cls(message, status=status, code=code, request_id=request_id)


def _retry_after(resp: httpx.Response) -> float | None:
    raw = resp.headers.get("retry-after")
    if not raw:
        return None
    try:
        return max(0.0, min(30.0, float(raw)))
    except ValueError:
        return None


class TokenBucket:
    """Lets `per_minute` calls through per minute, with a burst of half that."""

    def __init__(self, per_minute: int):
        self.rate = max(1, per_minute) / 60.0
        self.capacity = float(max(4, per_minute // 2))
        self.tokens = self.capacity
        self.stamp = time.monotonic()
        self._lock = asyncio.Lock()

    async def acquire(self) -> None:
        while True:
            async with self._lock:
                now = time.monotonic()
                self.tokens = min(self.capacity, self.tokens + (now - self.stamp) * self.rate)
                self.stamp = now
                if self.tokens >= 1:
                    self.tokens -= 1
                    return
                wait = (1 - self.tokens) / self.rate
            await asyncio.sleep(wait)

    def penalize(self, seconds: float) -> None:
        """After a 429, make everyone wait roughly `seconds` before the next call."""
        self.tokens = min(self.tokens, -seconds * self.rate)


class SarvamHTTP:
    def __init__(self, settings: Settings, *, transport: httpx.AsyncBaseTransport | None = None, max_attempts: int = 4):
        self.settings = settings
        self.max_attempts = max_attempts
        self._buckets = {
            "llm": TokenBucket(settings.rpm_llm),
            "speech": TokenBucket(settings.rpm_speech),
            "text": TokenBucket(settings.rpm_text),
        }
        self._gate = asyncio.Semaphore(max(2, settings.parallel_calls * 2))
        self._client = httpx.AsyncClient(
            base_url=settings.base_url,
            headers={"api-subscription-key": settings.api_key} if settings.api_key else {},
            timeout=httpx.Timeout(90.0, connect=10.0),
            limits=httpx.Limits(max_connections=24, max_keepalive_connections=12),
            transport=transport,
        )

    async def aclose(self) -> None:
        await self._client.aclose()

    @staticmethod
    def _backoff(attempt: int) -> float:
        return min(20.0, 1.0 * 2 ** (attempt - 1)) * random.uniform(0.6, 1.0)

    async def request(
        self,
        bucket: str,
        method: str,
        path: str,
        *,
        json: Any = None,
        data: dict | None = None,
        files: Any = None,
        timeout: float | None = None,
    ) -> httpx.Response:
        if not self.settings.api_key:
            raise AuthError("No Sarvam API key found. Add SARVAM_API_KEY to your .env file.", status=403, code="invalid_api_key_error")
        limiter = self._buckets[bucket]
        last_exc: Exception | None = None
        for attempt in range(1, self.max_attempts + 1):
            await limiter.acquire()
            try:
                async with self._gate:
                    resp = await self._client.request(method, path, json=json, data=data, files=files, timeout=timeout)
            except (httpx.TimeoutException, httpx.TransportError) as exc:
                last_exc = exc
                if attempt == self.max_attempts:
                    break
                await asyncio.sleep(self._backoff(attempt))
                continue
            if resp.status_code < 400:
                return resp
            err = _error_from(resp)
            retryable = resp.status_code in RETRY_STATUS and err.code not in NO_RETRY_CODES
            if retryable and attempt < self.max_attempts:
                delay = _retry_after(resp) or self._backoff(attempt)
                if resp.status_code == 429:
                    limiter.penalize(delay)
                await asyncio.sleep(delay)
                continue
            raise err
        reason = type(last_exc).__name__ if last_exc else "unknown"
        raise Unavailable(f"Couldn't reach Sarvam ({reason}). Check your connection and try again.") from last_exc
