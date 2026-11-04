"""Wires the clients, the audit engine and the explainer together, and runs audits as background jobs."""
from __future__ import annotations

import asyncio
import logging
import secrets

from .audit.engine import AuditFailed, Engine
from .audit.models import AuditRequest, Plan, Report
from .audit.orchestrator import Orchestrator
from .audit.targets import Target, build_target
from .budget import Budget, estimate_audit
from .catalog import DEPTHS
from .config import Settings
from .errors import describe
from .explain.generator import Explainer, Explanation
from .jobs import Job, JobBoard
from .sarvam import AuthError, ChatClient, OutOfCredits, SarvamError, SarvamHTTP, SpeechClient, TextClient, Speech
from .store import Store

log = logging.getLogger("vak")


class Services:
    def __init__(self, settings: Settings, *, transport=None):
        self.settings = settings
        self.store = Store(settings.db_path)
        self.budget = Budget(self.store, settings.budget)
        self.http = SarvamHTTP(settings, transport=transport)
        self.chat = ChatClient(self.http, self.store, self.budget)
        self.text = TextClient(self.http, settings, self.store, self.budget)
        self.speech = SpeechClient(self.http, settings, self.budget)
        self.orchestrator = Orchestrator(self.chat, settings)
        self.engine = Engine(settings, self.chat, self.text, self.budget)
        self.explainer = Explainer(self.chat, settings)
        self.jobs = JobBoard()
        self._slots = asyncio.Semaphore(settings.audits_in_parallel)
        self.store.audit_fail_orphans()
        self.store.cache_prune()

    async def aclose(self) -> None:
        for job in list(self.jobs._jobs.values()):
            if job.task and not job.task.done():
                job.task.cancel()
        await self.http.aclose()
        self.store.close()

    @property
    def local_only(self) -> bool:
        return self.settings.host in ("127.0.0.1", "::1", "localhost")

    # --- planning and estimates -------------------------------------------------------

    def estimate(self, plan: Plan, *, voice: bool = False, voice_seconds: float = 0.0) -> dict[str, dict]:
        billed = plan.target.kind == "sarvam"
        return {
            depth: estimate_audit(
                depth, prompt_chars=len(plan.prompt),
                target_model=plan.target.model if billed else self.settings.target_model,
                helper_model=self.settings.plan_model, target_is_billed=billed,
                voice=voice, voice_seconds=voice_seconds,
            ).as_dict()
            for depth in DEPTHS
        }

    async def make_plan(self, text: str, overrides: dict | None) -> dict:
        plan = await self.orchestrator.plan(text, overrides=overrides)
        return {"plan": plan.model_dump(exclude={"target": {"api_key"}}), "estimates": self.estimate(plan)}

    # --- running audits ------------------------------------------------------------------

    async def start_audit(self, req: AuditRequest) -> str:
        plan = req.plan
        limit = self.settings.max_prompt_chars
        if len(plan.prompt) > limit:
            raise AuditFailed(f"Please keep the question under {limit} characters. Longer ones make the audit slow and costly.")
        target = build_target(
            plan.target, chat=self.chat, allow_custom=self.settings.allow_custom_targets,
            allow_private=self.local_only, fresh=req.fresh,
        )
        est = self.estimate(plan, voice=req.voice)[plan.depth]
        cap = self.settings.audit_cost_cap_inr
        if est["total_inr"] > cap:
            raise AuditFailed(f"This run could cost about ₹{est['total_inr']:.0f}, which is above your ₹{cap:.0f} limit. Try a quicker depth.")
        self.budget.ensure({"llm": est["llm_inr"], "translate": est["translate_inr"], "tts": est["tts_inr"]})

        audit_id = secrets.token_hex(6)
        stored = req.model_dump()
        stored["plan"]["target"].pop("api_key", None)
        self.store.audit_create(audit_id, language=req.language, prompt=plan.prompt, target=target.label, depth=plan.depth, request=stored)
        job = self.jobs.create(audit_id)
        job.task = asyncio.create_task(self._run(job, req, target))
        return audit_id

    async def cancel(self, audit_id: str) -> bool:
        job = self.jobs.get(audit_id)
        if job and job.task and not job.task.done():
            job.task.cancel()
            return True
        return False

    async def _run(self, job: Job, req: AuditRequest, target: Target) -> None:
        audit_id = job.id
        try:
            if self._slots.locked():
                await job.emit("queued", {})
            async with self._slots:
                self.store.audit_update(audit_id, status="running")
                with self.budget.track() as tally:
                    await job.emit("stage", {"name": "engine", "state": "active"})
                    report = await self.engine.run(req, target, job.emit)
                    report.cost_inr = round(tally.inr, 3)
                    self.store.audit_update(audit_id, report=report.model_dump(), score=report.consistency_score)
                    await job.emit("report", report.model_dump())
                    await job.emit("stage", {"name": "engine", "state": "done"})

                    await job.emit("stage", {"name": "explain", "state": "active"})
                    explanation = await self.explainer.explain(report, req.language, seed=audit_id)
                    self._save_explanation(audit_id, explanation)
                    await job.emit("explanation", explanation.to_dict())
                    await job.emit("stage", {"name": "explain", "state": "done"})

                    await job.emit("stage", {"name": "output", "state": "active"})
                    if req.voice:
                        try:
                            speech = await self.speech.synthesize(explanation.text, req.language, speaker=req.voice_id)
                            await job.emit("audio", self._save_audio(audit_id, req.language, speech))
                        except (AuthError, OutOfCredits):
                            raise
                        except SarvamError:
                            await job.emit("notice", {"message": "I couldn't make the voice this time, but the written explanation is ready."})
                    await job.emit("stage", {"name": "output", "state": "done"})
                    self._set_cost(audit_id, tally.inr)
                    self.store.audit_update(audit_id, status="done")
                    await job.emit("done", {"id": audit_id, "cost_inr": round(tally.inr, 3)})
        except asyncio.CancelledError:
            self.store.audit_update(audit_id, status="cancelled")
            await job.emit("cancelled", {})
        except Exception as exc:
            status, code, message = describe(exc)
            if status == 500:
                log.exception("audit %s failed", audit_id)
            self.store.audit_update(audit_id, status="failed", error=message)
            await job.emit("error", {"code": code, "message": message})
        finally:
            await job.close()

    # --- explanation and speech on demand ---------------------------------------------------

    def _record(self, audit_id: str) -> dict:
        rec = self.store.audit_get(audit_id)
        if rec is None or not rec.get("report"):
            raise KeyError(audit_id)
        return rec

    def _save_explanation(self, audit_id: str, explanation: Explanation) -> None:
        rec = self.store.audit_get(audit_id) or {}
        saved = dict(rec.get("explanations") or {})
        saved[explanation.language] = explanation.to_dict()   # new text, so any older audio no longer matches
        self.store.audit_update(audit_id, explanations=saved)

    def _save_audio(self, audit_id: str, language: str, speech: Speech) -> dict:
        info = {
            "url": f"/api/audio/{speech.path.name}", "seconds": round(speech.seconds, 2),
            "speaker": speech.speaker, "cached": speech.cached, "language": language,
        }
        rec = self.store.audit_get(audit_id) or {}
        saved = dict(rec.get("explanations") or {})
        if language in saved:
            saved[language]["audio"] = info
            self.store.audit_update(audit_id, explanations=saved)
        return info

    def _set_cost(self, audit_id: str, inr: float) -> None:
        rec = self.store.audit_get(audit_id)
        if rec and rec.get("report"):
            report = rec["report"]
            report["cost_inr"] = round(max(report.get("cost_inr", 0.0), inr), 3)
            self.store.audit_update(audit_id, report=report)

    def _add_cost(self, audit_id: str, inr: float) -> None:
        rec = self.store.audit_get(audit_id)
        if rec and rec.get("report"):
            report = rec["report"]
            report["cost_inr"] = round(report.get("cost_inr", 0.0) + inr, 3)
            self.store.audit_update(audit_id, report=report)

    async def explanation_for(self, audit_id: str, language: str, *, force: bool = False) -> dict:
        rec = self._record(audit_id)
        existing = (rec.get("explanations") or {}).get(language)
        if existing and not force:
            return existing
        report = Report(**rec["report"])
        with self.budget.track() as tally:
            explanation = await self.explainer.explain(report, language, seed=audit_id + language)
        self._save_explanation(audit_id, explanation)
        self._add_cost(audit_id, tally.inr)
        return explanation.to_dict()

    async def speak(self, audit_id: str, language: str, *, voice_id: str | None, pace: float) -> dict:
        explanation = await self.explanation_for(audit_id, language)
        with self.budget.track() as tally:
            speech = await self.speech.synthesize(explanation["text"], language, speaker=voice_id, pace=pace)
        info = self._save_audio(audit_id, language, speech)
        if not speech.cached:
            self._add_cost(audit_id, tally.inr)
        return {**info, "cost_inr": round(speech.cost_inr, 3)}
