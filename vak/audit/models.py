"""Request, plan and report shapes. These are what cross the API boundary."""
from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field, field_validator

AuditType = Literal["consistency", "perturbation", "behavioral"]
Depth = Literal["quick", "standard", "deep"]
Lang = Literal["te", "hi", "en"]
ExplainLang = Literal["te", "hi"]

ALL_TYPES: list[AuditType] = ["consistency", "perturbation", "behavioral"]


class TargetSpec(BaseModel):
    """Which model gets audited."""
    kind: Literal["sarvam", "demo", "custom"] = "sarvam"
    model: str = "sarvam-105b-conversations"
    system_prompt: str | None = Field(default=None, max_length=1500)
    # custom OpenAI-compatible endpoint. The key lives in memory for the run and is never stored.
    base_url: str | None = None
    api_key: str | None = Field(default=None, repr=False)
    remote_model: str | None = None

    @field_validator("system_prompt")
    @classmethod
    def _blank_is_none(cls, v):
        return v.strip() or None if isinstance(v, str) else v

    def public(self) -> dict:
        """Safe to store and show: no key."""
        d = self.model_dump(exclude={"api_key"})
        return d


class Plan(BaseModel):
    prompt: str = Field(min_length=1)
    reference_output: str | None = None
    target: TargetSpec = Field(default_factory=TargetSpec)
    audit_types: list[AuditType] = Field(default_factory=lambda: list(ALL_TYPES))
    depth: Depth = "standard"
    prompt_language: Lang = "te"
    notes: list[str] = Field(default_factory=list)
    source_text: str = ""

    @field_validator("prompt")
    @classmethod
    def _strip_prompt(cls, v: str) -> str:
        v = v.strip()
        if not v:
            raise ValueError("The prompt to test can't be empty.")
        return v

    @field_validator("audit_types")
    @classmethod
    def _need_one(cls, v):
        return list(dict.fromkeys(v)) or list(ALL_TYPES)


class AuditRequest(BaseModel):
    plan: Plan
    language: ExplainLang = "te"
    voice: bool = False
    voice_id: str | None = None
    fresh: bool = False          # skip the answer cache for the audited model


# --- results ----------------------------------------------------------------

class TestResult(BaseModel):
    # Not a test case; stops pytest trying to collect the class.
    __test__ = False

    id: str
    category: str
    kind: Literal["invariance", "sensitivity", "resample"]
    label: str
    input: str
    output: str | None = None
    sim: float | None = None
    verdict: Literal["same", "partly", "different"] | None = None
    score: float | None = None
    status: Literal["ok", "wobble", "broke", "error"] = "error"
    judged: bool = False
    reason: str = ""
    note: str = ""


class Unit(BaseModel):
    index: int
    text: str
    start: int
    end: int
    importance: float = 0.0     # 0..1, relative to the most important unit
    raw: float = 0.0            # 1 - similarity after removing it
    changed: bool = False


class Flag(BaseModel):
    code: str
    severity: Literal["low", "medium", "high"]
    title: str
    detail: str
    evidence: list[str] = Field(default_factory=list)


class Faithfulness(BaseModel):
    status: Literal["ok", "inconclusive", "skipped"] = "skipped"
    score: float | None = None
    claimed: list[str] = Field(default_factory=list)
    matched: list[int] = Field(default_factory=list)
    top: list[int] = Field(default_factory=list)
    note: str = ""


class Group(BaseModel):
    key: str
    label: str
    score: float
    n: int


class Report(BaseModel):
    prompt: str
    prompt_language: Lang
    baseline_output: str
    reference_output: str | None = None
    target_label: str
    depth: Depth
    audit_types: list[AuditType]

    consistency_score: float
    grade: Literal["concerning", "mixed", "steady"]
    total_perturbations: int
    unexpected_changes: int
    skipped: int = 0

    tests: list[TestResult]
    groups: list[Group]
    units: list[Unit]
    faithfulness: Faithfulness
    flags: list[Flag]
    strengths: list[str] = Field(default_factory=list)
    next_steps: list[str]
    not_tested: list[str]
    notes: list[str] = Field(default_factory=list)

    judge_used: bool = True
    cost_inr: float = 0.0
    seconds: float = 0.0
