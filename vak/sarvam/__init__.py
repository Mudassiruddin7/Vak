from .chat import ChatClient, Completion
from .http import (
    AuthError,
    BadRequest,
    OutOfCredits,
    RateLimited,
    SarvamError,
    SarvamHTTP,
    Unavailable,
)
from .speech import SpeechClient, Speech, Transcript
from .text import TextClient

__all__ = [
    "AuthError", "BadRequest", "ChatClient", "Completion", "OutOfCredits", "RateLimited",
    "SarvamError", "SarvamHTTP", "Speech", "SpeechClient", "TextClient", "Transcript", "Unavailable",
]
