"""WAV handling and text chunking for speech."""
from __future__ import annotations

import base64
import io
import re
import wave

_SENTENCE_END = re.compile(r"(?<=[.!?।॥])\s+")


def wav_info(data: bytes) -> tuple[int, int, int, int]:
    """(sample rate, channels, sample width in bytes, frame count). Raises ValueError if not a WAV."""
    try:
        with wave.open(io.BytesIO(data)) as w:
            return w.getframerate(), w.getnchannels(), w.getsampwidth(), w.getnframes()
    except (wave.Error, EOFError) as exc:
        raise ValueError("not a readable WAV file") from exc


def wav_seconds(data: bytes) -> float:
    rate, _, _, frames = wav_info(data)
    return frames / float(rate) if rate else 0.0


def merge_wavs(parts: list[bytes], gap_ms: int = 220) -> bytes:
    """Join WAV clips that share a format, with a short silence between them."""
    if len(parts) == 1:
        return parts[0]
    out = io.BytesIO()
    params = None
    with wave.open(out, "wb") as dst:
        for i, blob in enumerate(parts):
            with wave.open(io.BytesIO(blob)) as src:
                fmt = (src.getnchannels(), src.getsampwidth(), src.getframerate())
                if params is None:
                    params = fmt
                    dst.setnchannels(fmt[0])
                    dst.setsampwidth(fmt[1])
                    dst.setframerate(fmt[2])
                elif fmt != params:
                    raise ValueError("clips have different formats")
                if i:
                    silence = int(fmt[2] * gap_ms / 1000) * fmt[0] * fmt[1]
                    dst.writeframes(b"\x00" * silence)
                dst.writeframes(src.readframes(src.getnframes()))
    return out.getvalue()


def decode_audios(audios: list[str]) -> bytes:
    """Sarvam returns base64 WAV strings. Usually there is one; handle several defensively."""
    if not audios:
        raise ValueError("no audio in response")
    if len(audios) == 1:
        return base64.b64decode(audios[0])
    pieces = [base64.b64decode(a) for a in audios]
    if all(p[:4] == b"RIFF" for p in pieces):
        return merge_wavs(pieces)
    return base64.b64decode("".join(audios))


def split_for_tts(text: str, limit: int = 1800) -> list[str]:
    """Break text into pieces under `limit` characters, at sentence ends where possible."""
    text = text.strip()
    if len(text) <= limit:
        return [text] if text else []
    chunks, current = [], ""
    for sentence in _SENTENCE_END.split(text):
        sentence = sentence.strip()
        if not sentence:
            continue
        while len(sentence) > limit:
            cut = max(sentence.rfind(",", 0, limit), sentence.rfind(" ", 0, limit))
            cut = cut if cut > limit // 2 else limit
            piece, sentence = sentence[:cut].strip(), sentence[cut:].strip(" ,")
            if current:
                chunks.append(current)
                current = ""
            chunks.append(piece)
        if current and len(current) + 1 + len(sentence) > limit:
            chunks.append(current)
            current = sentence
        else:
            current = f"{current} {sentence}".strip()
    if current:
        chunks.append(current)
    return chunks


_MARKUP = re.compile(r"[*_#`>~|]+")
_BIG_NUMBER = re.compile(r"(?<![\d,.])\d{5,}(?![\d,])")


def clean_for_tts(text: str) -> str:
    """Strip markup the voice would stumble on and put commas in long numbers (Sarvam's advice)."""
    text = _MARKUP.sub(" ", text)
    text = re.sub(r"^\s*[-•]\s+", "", text, flags=re.MULTILINE)
    text = _BIG_NUMBER.sub(lambda m: f"{int(m.group()):,}", text)
    return re.sub(r"[ \t]+", " ", re.sub(r"\n{2,}", "\n", text)).strip()
