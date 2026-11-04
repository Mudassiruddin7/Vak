"""Settings, read from the environment and from .env files.

Real environment variables win over .env, and a .env in the current folder
wins over the one next to the package.
"""
from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path

from dotenv import dotenv_values

from .catalog import DEFAULT_BUDGET_INR

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def _read_env() -> dict[str, str]:
    merged: dict[str, str] = {}
    files = []
    for base in (PROJECT_ROOT, Path.cwd()):
        candidate = base / ".env"
        if candidate.is_file() and candidate not in files:
            files.append(candidate)
    for path in files:
        merged.update({k: v for k, v in dotenv_values(path).items() if v is not None})
    merged.update(os.environ)
    return merged


def _num(env: dict, key: str, default: float) -> float:
    try:
        return float(env.get(key, default))
    except (TypeError, ValueError):
        return default


def _flag(env: dict, key: str, default: bool) -> bool:
    raw = env.get(key)
    if raw is None:
        return default
    return str(raw).strip().lower() not in {"0", "false", "no", "off", ""}


@dataclass(frozen=True)
class Settings:
    api_key: str = ""
    base_url: str = "https://api.sarvam.ai"
    data_dir: Path = PROJECT_ROOT / "data"
    host: str = "127.0.0.1"
    port: int = 8420

    stt_model: str = "saaras:v4"
    tts_model: str = "bulbul:v4-flash"
    translate_model: str = "mayura:v1"
    target_model: str = "sarvam-105b-conversations"
    plan_model: str = "sarvam-105b"
    judge_model: str = "sarvam-105b"
    explain_model: str = "sarvam-105b"

    rpm_llm: int = 48
    rpm_speech: int = 48
    rpm_text: int = 48
    parallel_calls: int = 6
    audits_in_parallel: int = 2

    max_prompt_chars: int = 700
    audit_cost_cap_inr: float = 15.0
    allow_custom_targets: bool = True
    budget: dict = field(default_factory=lambda: dict(DEFAULT_BUDGET_INR))

    @property
    def has_key(self) -> bool:
        return bool(self.api_key)

    @property
    def audio_dir(self) -> Path:
        return self.data_dir / "audio"

    @property
    def db_path(self) -> Path:
        return self.data_dir / "vak.db"

    @classmethod
    def from_env(cls) -> "Settings":
        env = _read_env()
        data_dir = Path(env.get("VAK_DATA_DIR") or PROJECT_ROOT / "data").expanduser()
        budget = {
            name: _num(env, f"VAK_BUDGET_{name.upper()}", default)
            for name, default in DEFAULT_BUDGET_INR.items()
        }
        d = cls()
        return cls(
            api_key=(env.get("SARVAM_API_KEY") or "").strip(),
            base_url=(env.get("VAK_SARVAM_BASE_URL") or d.base_url).rstrip("/"),
            data_dir=data_dir,
            host=env.get("VAK_HOST") or d.host,
            port=int(_num(env, "VAK_PORT", d.port)),
            stt_model=env.get("VAK_STT_MODEL") or d.stt_model,
            tts_model=env.get("VAK_TTS_MODEL") or d.tts_model,
            translate_model=env.get("VAK_TRANSLATE_MODEL") or d.translate_model,
            target_model=env.get("VAK_TARGET_MODEL") or d.target_model,
            plan_model=env.get("VAK_PLAN_MODEL") or d.plan_model,
            judge_model=env.get("VAK_JUDGE_MODEL") or d.judge_model,
            explain_model=env.get("VAK_EXPLAIN_MODEL") or d.explain_model,
            rpm_llm=int(_num(env, "VAK_RPM_LLM", d.rpm_llm)),
            rpm_speech=int(_num(env, "VAK_RPM_SPEECH", d.rpm_speech)),
            rpm_text=int(_num(env, "VAK_RPM_TEXT", d.rpm_text)),
            parallel_calls=int(_num(env, "VAK_PARALLEL_CALLS", d.parallel_calls)),
            audits_in_parallel=int(_num(env, "VAK_AUDITS_IN_PARALLEL", d.audits_in_parallel)),
            max_prompt_chars=int(_num(env, "VAK_MAX_PROMPT_CHARS", d.max_prompt_chars)),
            audit_cost_cap_inr=_num(env, "VAK_AUDIT_COST_CAP_INR", d.audit_cost_cap_inr),
            allow_custom_targets=_flag(env, "VAK_ALLOW_CUSTOM_TARGETS", True),
            budget=budget,
        )
