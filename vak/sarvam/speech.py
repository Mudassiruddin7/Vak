"""Speech in (Saaras) and speech out (Bulbul)."""
from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from ..audio_util import clean_for_tts, decode_audios, merge_wavs, split_for_tts, wav_seconds
from ..budget import Budget
from ..catalog import LANGS, STT_INR_PER_MIN, VOICES, bcp47, tts_cost_inr
from ..config import Settings
from ..util import digest
from .http import BadRequest, SarvamHTTP

MAX_CLIP_SECONDS = 30.0   # the synchronous speech-to-text endpoint refuses anything longer
MIN_CLIP_SECONDS = 0.4

# Names the speech model tends to mangle. Passing them nudges recognition, nothing more.
KEYTERMS = ["Sarvam", "Gemma", "DeepSeek", "prompt", "model", "Vak"]


@dataclass
class Transcript:
    text: str
    language: str | None      # "te", "hi", "en" or None when Sarvam couldn't tell
    seconds: float
    cost_inr: float


@dataclass
class Speech:
    path: Path
    seconds: float
    chars: int
    cost_inr: float
    cached: bool
    speaker: str


def short_lang(code: str | None) -> str | None:
    if not code:
        return None
    short = code.split("-")[0].lower()
    return short if short in LANGS else None


def default_voice(lang: str) -> str:
    return VOICES[lang][0].id


class SpeechClient:
    def __init__(self, http: SarvamHTTP, settings: Settings, budget: Budget):
        self.http = http
        self.settings = settings
        self.budget = budget

    async def transcribe(
        self,
        audio: bytes,
        *,
        filename: str = "clip.wav",
        content_type: str = "audio/wav",
        language: str = "auto",
    ) -> Transcript:
        try:
            seconds = wav_seconds(audio)
        except ValueError:
            seconds = len(audio) / 16000.0   # not a WAV; a rough guess is enough for the meter
        else:
            if seconds > MAX_CLIP_SECONDS:
                raise BadRequest("That recording is longer than 30 seconds. Try a shorter one.", status=413)
        if seconds < MIN_CLIP_SECONDS:
            raise BadRequest("I couldn't hear anything in that recording. Try again a little louder.", status=422)

        data = {
            "model": self.settings.stt_model,
            "language_code": bcp47(language) if language in LANGS else "unknown",
        }
        if self.settings.stt_model == "saaras:v4":
            data["keyterms"] = json.dumps(KEYTERMS)
        resp = await self.http.request(
            "speech", "POST", "/speech-to-text", data=data, files={"file": (filename, audio, content_type)},
        )
        body = resp.json()
        cost = seconds / 60.0 * STT_INR_PER_MIN
        self.budget.record("stt", round(seconds, 2), "seconds", cost, note=self.settings.stt_model)
        return Transcript(
            text=(body.get("transcript") or "").strip(),
            language=short_lang(body.get("language_code")),
            seconds=seconds,
            cost_inr=cost,
        )

    async def synthesize(self, text: str, lang: str, *, speaker: str | None = None, pace: float = 1.0) -> Speech:
        text = clean_for_tts(text)
        if not text:
            raise BadRequest("There is nothing to read aloud.", status=422)
        model = self.settings.tts_model
        speaker = speaker or default_voice(lang)
        pace = round(min(2.0, max(0.5, pace)), 2)
        name = digest("tts", model, lang, speaker, pace, text)[:24]
        path = self.settings.audio_dir / f"{name}.wav"
        if path.exists():
            return Speech(path, wav_seconds(path.read_bytes()), len(text), 0.0, True, speaker)

        parts = []
        for chunk in split_for_tts(text):
            parts.append(await self._speak_chunk(chunk, lang, model, speaker, pace))
        wav = merge_wavs(parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(wav)
        cost = tts_cost_inr(model, len(text))
        self.budget.record("tts", len(text), "chars", cost, note=f"{model}/{speaker}")
        return Speech(path, wav_seconds(wav), len(text), cost, False, speaker)

    async def _speak_chunk(self, chunk: str, lang: str, model: str, speaker: str, pace: float) -> bytes:
        body = {
            "text": chunk,
            "language_code": bcp47(lang),
            "model": model,
            "speaker": speaker,
            "pace": pace,
            "speech_sample_rate": 24000,
            "temperature": 0.45,
        }
        try:
            resp = await self.http.request("speech", "POST", "/text-to-speech", json=body)
        except BadRequest:
            fallback = default_voice(lang)
            if speaker == fallback:
                raise
            body["speaker"] = fallback
            resp = await self.http.request("speech", "POST", "/text-to-speech", json=body)
        return decode_audios(resp.json().get("audios") or [])
