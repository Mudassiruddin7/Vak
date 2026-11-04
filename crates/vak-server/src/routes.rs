use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::header;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use vak_adapters::custom::CustomTarget;
use vak_app::catalog::{chat_model, CHAT_MODELS};
use vak_app::engine::{AuditError, Engine};
use vak_app::explain::Explainer;
use vak_app::live::{Interpreter, Mode, TurnRequest, DOMAINS};
use vak_app::orchestrator::{Orchestrator, Overrides};
use vak_app::plan::{AuditRequest, Plan, TargetSpec};
use vak_app::ports::{emit, ApiError, Emit, Stt, Translator, TranslateOpts, Tts};
use vak_app::target::{practice_target, SarvamTarget, Target};
use vak_core::audit::{difficulty, AuditType, Effort, Report};
use vak_core::lang::{text_for, Lang, Tier};

use crate::state::{fail, spend_or_429, ApiFail, AppState, Principal};

type S = State<Arc<AppState>>;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/config", get(config))
        .route("/api/plan", post(plan))
        .route("/api/audits/stream", post(audit_stream))
        .route("/api/explain", post(explain))
        .route("/api/translate", post(translate))
        .route("/api/check", post(check))
        .route("/api/speak", post(speak))
        .route("/api/transcribe", post(transcribe).layer(DefaultBodyLimit::max(4_300_000)))
        .route("/api/live/turn", post(live_turn))
        .route("/api/live/utterance", post(live_utterance).layer(DefaultBodyLimit::max(4_300_000)))
        .route("/api/demo/{name}", get(demo))
        .layer(DefaultBodyLimit::max(96 * 1024))
        .with_state(state)
}

fn api_err(e: ApiError) -> ApiFail {
    match e {
        ApiError::Auth => fail(502, "Sarvam rejected the API key. If you added your own, check it in Settings."),
        ApiError::OutOfCredits => fail(402, "The Sarvam account behind this key has run out of credits."),
        ApiError::Timeout => fail(504, "Sarvam took too long to answer."),
        ApiError::Other(m) => fail(502, m),
    }
}

async fn health(State(s): S) -> Json<Value> {
    Json(json!({"ok": true, "server_key": s.sarvam.has_key(), "supabase": s.supa.is_some(), "version": env!("CARGO_PKG_VERSION")}))
}

fn lang_json(l: Lang) -> Value {
    let t = text_for(&l.code);
    json!({
        "code": l.code, "bcp47": l.bcp47, "english": l.english, "native": l.native, "script": l.script, "tier": l.tier,
        "voice": l.tts.is_some(), "explains": t.is_some(), "unreviewed": t.is_some_and(|t| t.machine_translated),
        "max_words": l.max_words, "sentence_end": l.sentence_end, "you": l.you, "register": l.register, "digits": l.digits,
        "script_notes": l.script_notes, "loanwords": l.loanwords,
        "notices": t.map(|t| json!(t.notices)),
    })
}

async fn config(State(s): S, who: Principal) -> Json<Value> {
    let presets: Vec<Value> = [Effort::Light, Effort::Balanced, Effort::Thorough].iter().map(|e| json!({"id": e.as_str(), "preset": e.preset()})).collect();
    Json(json!({
        "languages": Lang::all().map(lang_json).collect::<Vec<_>>(),
        "efforts": presets,
        "domains": DOMAINS,
        "models": CHAT_MODELS.iter().map(|m| json!({"id": m.id, "label": m.label, "blurb": m.blurb, "inr_in": m.inr_in, "inr_out": m.inr_out})).collect::<Vec<_>>(),
        "practice": vak_core::audit::practice::MODELS.iter().map(|(id, label)| json!({"id": id, "label": label})).collect::<Vec<_>>(),
        "supabase": s.supa.as_ref().map(|x| x.public_config()),
        "server_key": s.sarvam.has_key(),
        "custom_allowed": s.cfg.allow_custom,
        "quota_left": s.quota.left(&who).min(9999.0),
        "limits": {"prompt_chars": s.cfg.settings.max_prompt_chars, "turn_chars": vak_app::live::MAX_TURN_CHARS, "clip_seconds": 30, "cost_cap_inr": s.cfg.audit_cost_cap_inr},
    }))
}

fn units_for(e: Effort) -> f64 {
    match e {
        Effort::Light => 1.0,
        Effort::Balanced => 2.0,
        Effort::Thorough => 4.0,
    }
}

/// A rough price range before anything is spent: low when every kind of test comes back clean and the
/// adaptive plan stops early, high when everything runs.
fn estimate(plan: &Plan) -> Value {
    let p = plan.effort.preset();
    let chars = plan.prompt.chars().count() as f64;
    let full = p.typo + p.format + 2 + p.paraphrase + p.language + p.distractor + p.polite + p.negation + p.control + p.resamples;
    let kinds = [p.typo, p.format, 1, 1, p.paraphrase, p.language, p.distractor, p.polite, p.negation, p.control].iter().filter(|c| **c > 0).count();
    let first = if p.adaptive { (kinds * p.first_wave.min(2) + p.resamples.min(2)).min(full) } else { full };
    let units = p.units.min(chars as usize / 4 + 1);
    let price = |i: f64, o: f64| (i * 15.0 + o * 60.0) / 1_000_000.0;
    let target_call = price(chars / 3.0 + 40.0, 150.0);
    let translation = chars * (p.language as f64 + 1.0) / 1000.0 * 2.0;
    let helpers = price(350.0, 300.0) + price(950.0, 120.0) * 2.0 + price(1700.0, 700.0) * 1.3 + translation;
    let (low_calls, high_calls) = (first + units + 3, full + units + 3);
    let (low, high) = (low_calls as f64 * target_call + helpers * 0.8, high_calls as f64 * target_call + helpers * 1.2);
    json!({"calls_low": low_calls + 4, "calls_high": high_calls + 6, "inr_low": (low * 100.0).round() / 100.0, "inr_high": (high * 100.0).round() / 100.0, "units": units_for(plan.effort)})
}

#[derive(Deserialize)]
struct PlanBody {
    text: String,
    #[serde(default)]
    effort: Option<Effort>,
    #[serde(default)]
    audit_types: Option<Vec<AuditType>>,
    #[serde(default)]
    target: Option<TargetSpec>,
    #[serde(default)]
    reference_output: Option<String>,
}

async fn plan(State(s): S, who: Principal, Json(b): Json<PlanBody>) -> Result<Json<Value>, ApiFail> {
    if b.text.trim().is_empty() || b.text.chars().count() > 4000 {
        return Err(fail(400, "Say or type the question you want to test (up to 4000 characters)."));
    }
    spend_or_429(&s, &who, 0.1)?;
    let o = Orchestrator { llm: Arc::new(s.sarvam_for(&who)), settings: s.cfg.settings.clone() };
    let overrides = Overrides { effort: b.effort, audit_types: b.audit_types, target: b.target, reference_output: b.reference_output };
    let plan = o.plan(&b.text, &overrides).await.map_err(api_err)?.normalised().map_err(|m| fail(400, m))?;
    Ok(Json(json!({"plan": plan, "difficulty": difficulty(&plan.prompt), "estimate": estimate(&plan)})))
}

async fn build_target(s: &AppState, who: &Principal, spec: &TargetSpec, fresh: bool) -> Result<Arc<dyn Target>, ApiFail> {
    match spec.kind.as_str() {
        "demo" => practice_target(&spec.model).map(|t| Arc::new(t) as Arc<dyn Target>).ok_or_else(|| fail(400, "Unknown practice model.")),
        "custom" => {
            if !s.cfg.allow_custom {
                return Err(fail(403, "Auditing your own endpoint is switched off on this server."));
            }
            let (Some(url), Some(model)) = (&spec.base_url, &spec.remote_model) else {
                return Err(fail(400, "Give the endpoint URL and the model name to audit your own model."));
            };
            let t = CustomTarget::new(url, spec.api_key.clone(), model, spec.system_prompt.clone(), s.cfg.allow_private_targets).await.map_err(|m| fail(400, m))?;
            Ok(Arc::new(t))
        }
        _ => {
            chat_model(&spec.model).ok_or_else(|| fail(400, "Unknown model."))?;
            Ok(Arc::new(SarvamTarget { llm: Arc::new(s.sarvam_for(who)), model: spec.model.clone(), system: spec.system_prompt.clone(), use_cache: !fresh }))
        }
    }
}

fn sse(rx: tokio::sync::mpsc::UnboundedReceiver<(String, Value)>) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = UnboundedReceiverStream::new(rx).map(|(name, data)| Ok(Event::default().event(name).data(data.to_string())));
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(12)).text("."))
}

async fn audit_stream(State(s): S, who: Principal, Json(mut req): Json<AuditRequest>) -> Result<impl IntoResponse, ApiFail> {
    req.plan = req.plan.normalised().map_err(|m| fail(400, m))?;
    if req.plan.prompt.chars().count() > s.cfg.settings.max_prompt_chars * 2 {
        return Err(fail(400, "That question is too long to audit. Shorten it a little."));
    }
    let lang = Lang::get(&req.language).filter(|l| text_for(&l.code).is_some()).ok_or_else(|| fail(400, "Pick one of the explanation languages."))?;
    let target = build_target(&s, &who, &req.plan.target, req.fresh).await?;
    if who.own_key.is_none() && estimate(&req.plan)["inr_high"].as_f64().unwrap_or(0.0) > s.cfg.audit_cost_cap_inr {
        return Err(fail(400, "That audit would cost more than this server allows. Pick a lighter effort."));
    }
    spend_or_429(&s, &who, units_for(req.plan.effort))?;
    let permit = s.audits.clone().try_acquire_owned().map_err(|_| fail(503, "The server is busy with other audits. Try again in a few seconds."))?;
    let client = s.sarvam_for(&who);
    let engine = Engine { llm: Arc::new(client.clone()), translator: Arc::new(client.clone()), settings: s.cfg.settings.clone() };
    let explainer = Explainer { llm: Arc::new(client), model: s.cfg.settings.explain_model.clone() };
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let state = s.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let work = run_audit(&state, &engine, &explainer, &req, target, lang, tx.clone());
        tokio::select! { _ = work => {}, _ = tx.closed() => {} }
    });
    Ok(sse(rx))
}

async fn run_audit(s: &AppState, engine: &Engine, explainer: &Explainer, req: &AuditRequest, target: Arc<dyn Target>, lang: Lang, tx: Emit) {
    match engine.run(req, target, tx.clone()).await {
        Ok(report) => {
            let sig = s.sign(&serde_json::to_vec(&report).unwrap_or_default());
            emit(&tx, "report", json!({"report": report, "sig": sig}));
            emit(&tx, "phase", json!({"name": "explain"}));
            let p = Effort::parse(&report.effort).map(|e| e.preset());
            match explainer.explain(&report, lang, &report.prompt, p.map(|p| p.explain_attempts).unwrap_or(3), p.and_then(|p| p.judge_reasoning)).await {
                Ok(ex) => emit(&tx, "explanation", json!(ex)),
                Err(e) => emit(&tx, "explain_failed", json!({"message": api_err(e).1})),
            }
            emit(&tx, "done", json!({}));
        }
        Err(AuditError::Failed(m)) => emit(&tx, "error", json!({"message": m})),
        Err(AuditError::Api(e)) => {
            let f = api_err(e.clone());
            emit(&tx, "error", json!({"message": f.1, "code": f.0.as_u16()}));
        }
    }
}

#[derive(Deserialize)]
struct ExplainBody {
    report: Report,
    sig: String,
    language: String,
}

async fn explain(State(s): S, who: Principal, Json(b): Json<ExplainBody>) -> Result<Json<Value>, ApiFail> {
    if !s.verify(&serde_json::to_vec(&b.report).unwrap_or_default(), &b.sig) {
        return Err(fail(400, "That report wasn't made here, so I won't explain it."));
    }
    let lang = Lang::get(&b.language).filter(|l| text_for(&l.code).is_some()).ok_or_else(|| fail(400, "Pick one of the explanation languages."))?;
    spend_or_429(&s, &who, 0.4)?;
    let ex = Explainer { llm: Arc::new(s.sarvam_for(&who)), model: s.cfg.settings.explain_model.clone() };
    let attempts = Effort::parse(&b.report.effort).map(|e| e.preset().explain_attempts).unwrap_or(2);
    Ok(Json(json!(ex.explain(&b.report, lang, &b.report.prompt, attempts, None).await.map_err(api_err)?)))
}

#[derive(Deserialize)]
struct SpeakBody {
    text: String,
    lang: String,
    #[serde(default)]
    gender: Option<String>,
}

fn wav_response(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "audio/wav"), (header::CACHE_CONTROL, "no-store")], bytes).into_response()
}

async fn speak(State(s): S, who: Principal, Json(b): Json<SpeakBody>) -> Result<Response, ApiFail> {
    let n = b.text.chars().count();
    if n == 0 || n > 1800 {
        return Err(fail(400, "Give me between 1 and 1800 characters to read aloud."));
    }
    let l = Lang::get(&b.lang).filter(|l| l.tier == Tier::Full).ok_or_else(|| fail(400, "There is no voice for that language yet."))?;
    spend_or_429(&s, &who, 0.1 + n as f64 / 4000.0)?;
    let wav = s.sarvam_for(&who).speak(&b.text, &l.code, b.gender.as_deref().unwrap_or("female")).await.map_err(api_err)?;
    Ok(wav_response(wav))
}

async fn read_audio(mut mp: Multipart) -> Result<(Vec<u8>, String, std::collections::HashMap<String, String>), ApiFail> {
    let (mut audio, mut mime, mut fields) = (None, String::from("audio/wav"), std::collections::HashMap::new());
    while let Some(f) = mp.next_field().await.map_err(|_| fail(400, "The upload was unreadable."))? {
        let name = f.name().unwrap_or("").to_string();
        if name == "audio" {
            mime = f.content_type().unwrap_or("audio/wav").to_string();
            if !mime.starts_with("audio/") {
                return Err(fail(400, "That isn't an audio file."));
            }
            audio = Some(f.bytes().await.map_err(|_| fail(413, "That recording is too large."))?.to_vec());
        } else if name.len() < 24 {
            fields.insert(name, f.text().await.unwrap_or_default().chars().take(64).collect());
        }
    }
    Ok((audio.ok_or_else(|| fail(400, "No audio came with that request."))?, mime, fields))
}

async fn transcribe(State(s): S, who: Principal, mp: Multipart) -> Result<Json<Value>, ApiFail> {
    let (audio, mime, f) = read_audio(mp).await?;
    spend_or_429(&s, &who, 0.2)?;
    let t = s.sarvam_for(&who).transcribe(audio, &mime, f.get("lang").map(|s| s.as_str())).await.map_err(api_err)?;
    Ok(Json(json!({"text": t.text, "lang": t.lang})))
}

#[derive(Deserialize)]
struct TurnBody {
    text: String,
    src: String,
    dst: String,
    #[serde(default)]
    gender: Option<String>,
    #[serde(default)]
    careful: bool,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    native_numerals: bool,
}

fn to_turn(b: TurnBody) -> TurnRequest {
    TurnRequest {
        text: b.text,
        src: b.src,
        dst: b.dst,
        gender: b.gender.filter(|g| matches!(g.as_str(), "Male" | "Female")),
        mode: if b.careful { Mode::Careful } else { Mode::Fast },
        domain: b.domain,
        native_numerals: b.native_numerals,
    }
}

async fn live_turn(State(s): S, who: Principal, Json(b): Json<TurnBody>) -> Result<Json<Value>, ApiFail> {
    spend_or_429(&s, &who, if b.careful { 0.35 } else { 0.2 })?;
    let i = Interpreter { translator: Arc::new(s.sarvam_for(&who)) };
    Ok(Json(json!(i.turn(&to_turn(b)).await.map_err(api_err)?)))
}

async fn live_utterance(State(s): S, who: Principal, mp: Multipart) -> Result<Json<Value>, ApiFail> {
    let (audio, mime, f) = read_audio(mp).await?;
    let careful = f.get("careful").is_some_and(|v| v == "1");
    spend_or_429(&s, &who, if careful { 0.55 } else { 0.4 })?;
    let client = s.sarvam_for(&who);
    let (src, dst) = (f.get("src").cloned().unwrap_or_default(), f.get("dst").cloned().unwrap_or_default());
    let heard = client.transcribe(audio, &mime, Some(&src)).await.map_err(api_err)?;
    if heard.text.trim().is_empty() {
        return Ok(Json(json!({"heard": "", "empty": true})));
    }
    let body = TurnBody { text: heard.text.clone(), src, dst, gender: f.get("gender").cloned(), careful, domain: f.get("domain").cloned().unwrap_or_default(), native_numerals: f.get("native_numerals").is_some_and(|v| v == "1") };
    let turn = Interpreter { translator: Arc::new(client) }.turn(&to_turn(body)).await.map_err(api_err)?;
    Ok(Json(json!({"heard": heard.text, "detected": heard.lang, "turn": turn})))
}

async fn demo(Path(name): Path<String>) -> Result<Json<Value>, ApiFail> {
    // recorded audits ship with the front end; this only exists so the app can fetch them by name
    let raw = match name.as_str() {
        "steady" => include_str!("../../../web/demo/steady.json"),
        "shaky" => include_str!("../../../web/demo/shaky.json"),
        _ => return Err(fail(404, "No such example.")),
    };
    serde_json::from_str(raw).map(Json).map_err(|_| fail(500, "The example is damaged."))
}

#[derive(Deserialize)]
struct CheckBody {
    text: String,
    lang: String,
}

/// The writing rules, as a checker anyone can paste text into. Pure code: it costs nothing to run.
async fn check(Json(b): Json<CheckBody>) -> Result<Json<Value>, ApiFail> {
    if b.text.chars().count() > 6000 {
        return Err(fail(400, "Paste up to 6000 characters."));
    }
    let l = Lang::get(&b.lang).ok_or_else(|| fail(400, "Unknown language."))?;
    let v = vak_core::lang::check(&b.text, l, None);
    Ok(Json(json!({
        "ok": v.ok(), "banned": v.banned, "long_sentences": v.long_sentences, "wrong_script": v.wrong_script,
        "latin_heavy": v.latin_heavy, "sentences": v.sentence_count, "max_words": l.max_words,
    })))
}

#[derive(Deserialize)]
struct TranslateBody {
    text: String,
    #[serde(default)]
    src: String,
    dst: String,
    #[serde(default)]
    gender: Option<String>,
    /// Typed in English letters (like on a phone) in the source language: convert to its script first.
    #[serde(default)]
    romanized: bool,
    #[serde(default)]
    native_numerals: bool,
    /// Also return a second wording in the other register.
    #[serde(default)]
    alternatives: bool,
}

/// Text translation between any two of the 23 languages, with auto-detect, phone-style romanised input and
/// two registers, the way a dedicated translator works.
async fn translate(State(s): S, who: Principal, Json(b): Json<TranslateBody>) -> Result<Json<Value>, ApiFail> {
    let text = b.text.trim();
    let n = text.chars().count();
    if n == 0 || n > 2000 {
        return Err(fail(400, "Type or paste between 1 and 2000 characters."));
    }
    let dst = Lang::get(&b.dst).ok_or_else(|| fail(400, "Unknown target language."))?;
    let mut src = if b.src.is_empty() || b.src == "auto" { vak_core::lang::detect(text) } else { Lang::get(&b.src) };
    let mut input = text.to_string();
    let client = s.sarvam_for(&who);
    spend_or_429(&s, &who, 0.12 + n as f64 / 8000.0 + if b.alternatives { 0.1 } else { 0.0 })?;
    if b.romanized {
        let from = src.filter(|l| l.script != vak_core::text::Script::Latin).or_else(|| Lang::get("hi")).expect("hindi exists");
        // English letters to the language's own script, then translate that
        input = client.unromanize(&input, &from.code).await.map_err(api_err)?;
        src = Some(from);
    }
    let src = src.ok_or_else(|| fail(400, "I couldn't tell which language that is. Pick it from the list."))?;
    if src == dst {
        return Ok(Json(json!({"translation": input, "detected": src.code, "same": true, "input": input})));
    }
    let gender = b.gender.filter(|g| matches!(g.as_str(), "Male" | "Female"));
    let run = |mode: &'static str| {
        let opts = TranslateOpts { mode: Some(mode.into()), speaker_gender: gender.clone(), native_numerals: b.native_numerals };
        let (c, i, sc, dc) = (client.clone(), input.clone(), src.code.clone(), dst.code.clone());
        async move { c.translate(&i, &sc, &dc, &opts).await }
    };
    let (main, alt) = if b.alternatives { let (a, b2) = tokio::join!(run("formal"), run("modern-colloquial")); (a.map_err(api_err)?, b2.ok()) } else { (run("formal").await.map_err(api_err)?, None) };
    let alt = alt.filter(|a| vak_core::text::norm(a) != vak_core::text::norm(&main));
    Ok(Json(json!({"translation": main, "alternative": alt, "detected": src.code, "input": input, "can_speak": dst.tier == Tier::Full})))
}
