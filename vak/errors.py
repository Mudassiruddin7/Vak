"""Turns exceptions into something a person can act on: (HTTP status, short code, message)."""
from __future__ import annotations

from .audit.engine import AuditFailed
from .audit.targets import TargetError
from .budget import BudgetExhausted
from .sarvam import AuthError, BadRequest, OutOfCredits, RateLimited, Unavailable


def describe(exc: Exception) -> tuple[int, str, str]:
    if isinstance(exc, OutOfCredits):
        return 402, "out_of_credits", "Sarvam says the account has no credits left. Add some on the Sarvam dashboard and try again."
    if isinstance(exc, AuthError):
        return 502, "bad_key", "Sarvam didn't accept the API key. Check SARVAM_API_KEY in your .env file."
    if isinstance(exc, RateLimited):
        return 429, "busy", "Sarvam is getting a lot of requests right now. Give it a few seconds and try again."
    if isinstance(exc, Unavailable):
        return 503, "unavailable", exc.message
    if isinstance(exc, BadRequest):
        return (exc.status if exc.status in (413, 422) else 400), "bad_request", exc.message
    if isinstance(exc, BudgetExhausted):
        return 402, "budget", str(exc)
    if isinstance(exc, (AuditFailed, TargetError)):
        return 422, "cannot_run", str(exc)
    return 500, "internal", "Something unexpected went wrong. The server log has the details."
