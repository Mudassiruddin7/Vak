"""The web app: JSON API, a live event stream per audit, and the static front end."""
from __future__ import annotations

import re
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

from fastapi import FastAPI, File, Form, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, JSONResponse, PlainTextResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

from . import __version__
from .audit.demo import DEMOS
from .audit.engine import AuditFailed
from .audit.models import AuditRequest, ExplainLang, Plan
from .audit.targets import TargetError
from .budget import BudgetExhausted
from .catalog import CHAT_MODELS, DEPTHS, LANGS, VOICES
from .config import Settings
from .errors import describe
from .explain import fallback
from .export import report_markdown
from .sarvam import SarvamError
from .service import Services

WEB_DIR = Path(__file__).parent / "web"
MAX_UPLOAD_BYTES = 6 * 1024 * 1024
AUDIO_NAME = re.compile(r"^[0-9a-f]{24}\.wav$")

SAMPLES = [
    {"id": "rice", "lang": "te", "text": "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత?", "target": None,
     "label": {"en": "Rice price sum", "te": "బియ్యం లెక్క", "hi": "चावल का हिसाब"}},
    {"id": "train", "lang": "hi", "text": "दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?", "target": None,
     "label": {"en": "Train journey", "te": "రైలు ప్రయాణం", "hi": "ट्रेन का सफ़र"}},
    {"id": "film-shaky", "lang": "te", "text": "చెత్త సినిమా! డబ్బులు వృథా అయ్యాయి.", "target": {"kind": "demo", "model": "mood-shaky"},
     "label": {"en": "Film review · shaky practice model", "te": "సినిమా సమీక్ష · తడబడే మోడల్", "hi": "फ़िल्म की राय · डगमग मॉडल"}},
    {"id": "film-steady", "lang": "hi", "text": "यह फ़िल्म बहुत अच्छी है, कहानी शानदार है।", "target": {"kind": "demo", "model": "mood-steady"},
     "label": {"en": "Film review · steady practice model", "te": "సినిమా సమీక్ష · స్థిరమైన మోడల్", "hi": "फ़िल्म की राय · स्थिर मॉडल"}},
]


class PlanBody(BaseModel):
    text: str = Field(min_length=1, max_length=4000)
    overrides: dict | None = None


class EstimateBody(BaseModel):
    plan: Plan
    voice: bool = False


class ExplainBody(BaseModel):
    language: ExplainLang
    force: bool = False


class SpeakBody(BaseModel):
    language: ExplainLang
    voice_id: str | None = None
    pace: float = Field(default=1.0, ge=0.5, le=2.0)


class NoCacheStatic(StaticFiles):
    async def get_response(self, path, scope):
        response = await super().get_response(path, scope)
        response.headers["Cache-Control"] = "no-cache"
        return response


def create_app(settings: Settings | None = None, *, transport=None) -> FastAPI:
    settings = settings or Settings.from_env()
    svc = Services(settings, transport=transport)

    @asynccontextmanager
    async def lifespan(_: FastAPI):
        yield
        await svc.aclose()

    app = FastAPI(title="Vak", version=__version__, lifespan=lifespan,
                  docs_url="/api/docs", redoc_url=None, openapi_url="/api/openapi.json")
    app.state.svc = svc

    async def known_error(_: Request, exc: Exception):
        status, code, message = describe(exc)
        return JSONResponse(status_code=status, content={"error": {"code": code, "message": message}})

    for kind in (SarvamError, AuditFailed, TargetError, BudgetExhausted):
        app.add_exception_handler(kind, known_error)

    def need_record(audit_id: str) -> dict:
        rec = svc.store.audit_get(audit_id)
        if rec is None:
            raise HTTPException(404, "I can't find that audit.")
        return rec

    # --- basics ------------------------------------------------------------------------

    @app.get("/api/health")
    def health():
        return {"ok": True, "version": __version__, "has_key": settings.has_key}

    @app.get("/api/config")
    def config():
        return {
            "version": __version__,
            "has_key": settings.has_key,
            "languages": {k: {"english": v.english, "native": v.native} for k, v in LANGS.items()},
            "depths": list(DEPTHS),
            "targets": {
                "sarvam": [{"id": m.id, "label": m.label, "blurb": m.blurb} for m in CHAT_MODELS.values()],
                "demo": [{"id": k, "label": v} for k, v in DEMOS.items()],
                "custom_allowed": settings.allow_custom_targets,
                "default": settings.target_model,
            },
            "voices": {lang: [{"id": v.id, "name": v.name, "gender": v.gender} for v in voices] for lang, voices in VOICES.items()},
            "findings": {
                lang: {"flags": bank["flags"], "steps": bank["steps"], "not_tested": bank["not_tested"]}
                for lang, bank in (("te", fallback.TE), ("hi", fallback.HI))
            },
            "samples": SAMPLES,
            "limits": {"prompt_chars": settings.max_prompt_chars, "clip_seconds": 30, "cost_cap_inr": settings.audit_cost_cap_inr},
        }

    @app.get("/api/budget")
    def budget():
        return svc.budget.summary()

    @app.get("/api/history")
    def history():
        return svc.store.audit_list(40)

    # --- listening and planning ----------------------------------------------------------

    @app.post("/api/transcribe")
    async def transcribe(audio: UploadFile = File(...), language: str = Form("auto")):
        data = await audio.read(MAX_UPLOAD_BYTES + 1)
        if len(data) > MAX_UPLOAD_BYTES:
            raise HTTPException(413, "That recording is too large.")
        result = await svc.speech.transcribe(
            data, filename=audio.filename or "clip.wav", content_type=audio.content_type or "audio/wav", language=language,
        )
        if not result.text:
            raise HTTPException(422, "I couldn't make out any words. Try again a little closer to the mic.")
        return {"text": result.text, "language": result.language, "seconds": round(result.seconds, 1), "cost_inr": round(result.cost_inr, 4)}

    @app.post("/api/plan")
    async def plan(body: PlanBody):
        return await svc.make_plan(body.text, body.overrides)

    @app.post("/api/estimate")
    def estimate(body: EstimateBody):
        return {"estimates": svc.estimate(body.plan, voice=body.voice)}

    # --- audits -----------------------------------------------------------------------------

    @app.post("/api/audits", status_code=202)
    async def start(req: AuditRequest):
        return {"id": await svc.start_audit(req)}

    @app.get("/api/audits/{audit_id}")
    def get_audit(audit_id: str):
        return need_record(audit_id)

    @app.delete("/api/audits/{audit_id}", status_code=204)
    def delete_audit(audit_id: str):
        if not svc.store.audit_delete(audit_id):
            raise HTTPException(404, "I can't find that audit.")

    @app.get("/api/audits/{audit_id}/events")
    async def events(audit_id: str, request: Request, since: int = 0):
        job = svc.jobs.get(audit_id)
        if job is None:
            raise HTTPException(404, "That audit isn't running any more. Open it from your history instead.")
        last = request.headers.get("last-event-id", "")
        start_at = int(last) + 1 if last.isdigit() else since
        return StreamingResponse(
            job.stream(start_at), media_type="text/event-stream",
            headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
        )

    @app.post("/api/audits/{audit_id}/cancel")
    async def cancel(audit_id: str):
        return {"cancelled": await svc.cancel(audit_id)}

    @app.post("/api/audits/{audit_id}/explain")
    async def explain(audit_id: str, body: ExplainBody):
        need_record(audit_id)
        return await svc.explanation_for(audit_id, body.language, force=body.force)

    @app.post("/api/audits/{audit_id}/speak")
    async def speak(audit_id: str, body: SpeakBody):
        need_record(audit_id)
        return await svc.speak(audit_id, body.language, voice_id=body.voice_id, pace=body.pace)

    @app.get("/api/audits/{audit_id}/report.md")
    def markdown(audit_id: str):
        rec = need_record(audit_id)
        if not rec.get("report"):
            raise HTTPException(409, "This audit hasn't finished yet.")
        return PlainTextResponse(
            report_markdown(rec), media_type="text/markdown; charset=utf-8",
            headers={"Content-Disposition": f'attachment; filename="vak-audit-{audit_id}.md"'},
        )

    @app.get("/api/audio/{name}")
    def audio(name: str):
        path = settings.audio_dir / name
        if not AUDIO_NAME.match(name) or not path.is_file():
            raise HTTPException(404, "No such recording.")
        return FileResponse(path, media_type="audio/wav", headers={"Cache-Control": "public, max-age=86400"})

    WEB_DIR.mkdir(exist_ok=True)
    app.mount("/", NoCacheStatic(directory=WEB_DIR, html=True), name="web")
    return app
