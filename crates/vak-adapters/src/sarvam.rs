//! Sarvam's APIs behind the traits the use cases ask for: auth header, a client-side rate limit, retries
//! that follow Sarvam's own guidance, and an answer cache for deterministic calls.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use vak_app::catalog::chat_api_for;
use vak_app::ports::*;
use vak_core::lang::{self, Lang};
use vak_core::text::split_sentences;

use crate::cache::Cache;
use crate::wav;

const TIMEOUT: Duration = Duration::from_secs(45);
const MAX_ATTEMPTS: u32 = 4;
const MAX_TIMEOUTS: u32 = 2;
pub const MAX_CLIP_SECONDS: f64 = 30.0;
const TTS_CHUNK: usize = 2200;
const TRANSLATE_CHUNK: usize = 1800;

/// Lets `per_minute` calls through per minute, with a burst of half that.
pub struct Limiter {
    rate: f64,
    cap: f64,
    state: Mutex<(f64, Instant)>,
}

impl Limiter {
    pub fn new(per_minute: u32) -> Self {
        let cap = (per_minute / 2).max(4) as f64;
        Limiter { rate: per_minute.max(1) as f64 / 60.0, cap, state: Mutex::new((cap, Instant::now())) }
    }

    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut s = self.state.lock().await;
                let now = Instant::now();
                s.0 = (s.0 + now.duration_since(s.1).as_secs_f64() * self.rate).min(self.cap);
                s.1 = now;
                if s.0 >= 1.0 {
                    s.0 -= 1.0;
                    return;
                }
                (1.0 - s.0) / self.rate
            };
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
        }
    }

    /// After a 429, make everyone wait roughly `secs` before the next call.
    pub async fn penalize(&self, secs: f64) {
        let mut s = self.state.lock().await;
        s.0 = s.0.min(-secs * self.rate);
    }
}

#[derive(Clone)]
pub struct Sarvam {
    http: reqwest::Client,
    key: String,
    base: String,
    limiter: Arc<Limiter>,
    cache: Arc<Cache<ChatReply>>,
    pub stt_model: String,
    pub translate_model: String,
    pub allow_cache: bool,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(8)
        .user_agent("vak/0.2")
        .build()
        .expect("http client builds")
}

impl Sarvam {
    pub fn new(key: &str, base: &str, rpm: u32) -> Self {
        Sarvam {
            http: http_client(),
            key: key.to_string(),
            base: base.trim_end_matches('/').to_string(),
            limiter: Arc::new(Limiter::new(rpm)),
            cache: Arc::new(Cache::new(2000, Duration::from_secs(3600))),
            stt_model: "saaras:v4".into(),
            translate_model: "sarvam-translate:v1".into(),
            allow_cache: true,
        }
    }

    /// The same client with a different key (a visitor's own). It gets its own rate limit and no shared cache,
    /// so one person's answers are never served to another.
    pub fn with_key(&self, key: &str, rpm: u32) -> Sarvam {
        Sarvam { key: key.to_string(), limiter: Arc::new(Limiter::new(rpm)), cache: Arc::new(Cache::new(200, Duration::from_secs(600))), ..self.clone() }
    }

    pub fn has_key(&self) -> bool {
        !self.key.is_empty()
    }

    fn backoff(attempt: u32) -> Duration {
        let base = (1u64 << (attempt - 1)).min(20) as f64;
        Duration::from_secs_f64(base * (0.6 + 0.4 * (Instant::now().elapsed().subsec_nanos() % 1000) as f64 / 1000.0))
    }

    async fn send(&self, path: &str, build: impl Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder) -> Result<reqwest::Response, ApiError> {
        if self.key.is_empty() {
            return Err(ApiError::Auth);
        }
        let url = format!("{}{}", self.base, path);
        let mut timeouts = 0;
        let mut last = ApiError::Other("The request failed.".into());
        for attempt in 1..=MAX_ATTEMPTS {
            self.limiter.acquire().await;
            let rb = build(self.http.post(&url).header("api-subscription-key", &self.key));
            let resp = match rb.send().await {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        timeouts += 1;
                        last = ApiError::Timeout;
                    } else {
                        last = ApiError::Other("Couldn't reach Sarvam.".into());
                    }
                    if attempt == MAX_ATTEMPTS || timeouts >= MAX_TIMEOUTS {
                        break;
                    }
                    tokio::time::sleep(Self::backoff(attempt)).await;
                    continue;
                }
            };
            let status = resp.status().as_u16();
            if status < 400 {
                return Ok(resp);
            }
            let retry_after = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok()).map(|s| s.clamp(0.0, 30.0));
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            let code = body.pointer("/error/code").and_then(|c| c.as_str()).unwrap_or("");
            let message = body.pointer("/error/message").and_then(|m| m.as_str()).or_else(|| body.get("detail").and_then(|d| d.as_str())).unwrap_or("").to_string();
            let err = match status {
                402 => ApiError::OutOfCredits,
                401 | 403 => ApiError::Auth,
                _ => ApiError::Other(if message.is_empty() { format!("Sarvam answered HTTP {status}.") } else { message }),
            };
            if code == "insufficient_quota_error" || code == "seat_budget_exhausted_error" {
                return Err(ApiError::OutOfCredits);
            }
            let retryable = matches!(status, 429 | 500 | 502 | 503 | 504);
            if retryable && attempt < MAX_ATTEMPTS {
                let delay = retry_after.map(Duration::from_secs_f64).unwrap_or_else(|| Self::backoff(attempt));
                if status == 429 {
                    self.limiter.penalize(delay.as_secs_f64()).await;
                }
                tokio::time::sleep(delay).await;
                last = err;
                continue;
            }
            return Err(err);
        }
        Err(last)
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        let resp = self.send(path, |rb| rb.json(body)).await?;
        resp.json::<Value>().await.map_err(|_| ApiError::Other("Sarvam sent back something unreadable.".into()))
    }
}

fn bcp47(code: &str) -> String {
    Lang::get(code).map(|l| l.bcp47.clone()).unwrap_or_else(|| "en-IN".into())
}

fn off_switch(model: &str) -> Option<(&'static str, Value)> {
    match model {
        "sarvam-105b" => Some(("reasoning_effort", Value::Null)),
        "deepseekv4.1-flash-1017" => Some(("reasoning_effort", json!("none"))),
        _ => None,
    }
}

#[async_trait]
impl Llm for Sarvam {
    async fn chat(&self, req: ChatRequest) -> Result<ChatReply, ApiError> {
        let api = chat_api_for(&req.model);
        let mut body = json!({
            "model": req.model,
            "messages": req.messages,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens.min(4096),
        });
        if req.n > 1 {
            body["n"] = json!(req.n);
        }
        if req.json {
            body["response_format"] = json!({"type": "json_object"});
        }
        match (&req.reasoning, off_switch(&req.model)) {
            (Some(r), Some(_)) => body["reasoning_effort"] = json!(r),
            (None, Some((k, v))) => body[k] = v,
            _ => {}
        }
        let cacheable = req.cache && self.allow_cache && req.temperature == 0.0;
        let key = hex::encode(Sha256::digest(format!("{api}{body}").as_bytes()));
        if cacheable {
            if let Some(mut hit) = self.cache.get(&key) {
                hit.cached = true;
                return Ok(hit);
            }
        }
        let data = self.post_json(&format!("/{api}/chat/completions"), &body).await?;
        let choices = data.get("choices").and_then(|c| c.as_array()).filter(|c| !c.is_empty()).ok_or_else(|| ApiError::Other("The model sent back an empty reply.".into()))?;
        let texts: Vec<String> = choices.iter().map(|c| vak_core::text::strip_think(c.pointer("/message/content").and_then(|t| t.as_str()).unwrap_or(""))).collect();
        let usage = data.get("usage");
        let reply = ChatReply {
            tokens_in: usage.and_then(|u| u.get("prompt_tokens")).and_then(|t| t.as_u64()).unwrap_or(0),
            tokens_out: usage.and_then(|u| u.get("completion_tokens")).and_then(|t| t.as_u64()).unwrap_or(0),
            texts,
            cached: false,
        };
        if cacheable && reply.texts.iter().any(|t| !t.is_empty()) {
            self.cache.put(key, reply.clone());
        }
        Ok(reply)
    }
}

fn pieces(text: &str, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for s in split_sentences(text) {
        match out.last_mut() {
            Some(last) if last.chars().count() + s.chars().count() < limit => {
                last.push(' ');
                last.push_str(&s);
            }
            _ => out.push(s.chars().take(limit).collect()),
        }
    }
    out
}

#[async_trait]
impl Translator for Sarvam {
    async fn translate(&self, text: &str, src: &str, dst: &str, opts: &TranslateOpts) -> Result<String, ApiError> {
        let text = text.trim();
        if text.is_empty() || src == dst {
            return Ok(text.to_string());
        }
        let mut out = vec![];
        for piece in pieces(text, TRANSLATE_CHUNK) {
            let mut body = json!({
                "input": piece,
                "source_language_code": bcp47(src),
                "target_language_code": bcp47(dst),
                "model": self.translate_model,
                "numerals_format": if opts.native_numerals { "native" } else { "international" },
            });
            if let Some(m) = &opts.mode {
                body["mode"] = json!(m);
            }
            if let Some(g) = &opts.speaker_gender {
                body["speaker_gender"] = json!(g);
            }
            let data = match self.post_json("/translate", &body).await {
                Ok(d) => d,
                // an option the model doesn't take: try again with the plain request
                Err(ApiError::Other(_)) if opts.mode.is_some() || opts.speaker_gender.is_some() => {
                    let plain = json!({"input": piece, "source_language_code": bcp47(src), "target_language_code": bcp47(dst), "model": self.translate_model});
                    self.post_json("/translate", &plain).await?
                }
                Err(e) => return Err(e),
            };
            let t = data.get("translated_text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
            if t.is_empty() {
                return Err(ApiError::Other("The translator sent back nothing.".into()));
            }
            out.push(t);
        }
        Ok(out.join(" "))
    }

    async fn romanize(&self, text: &str, src: &str) -> Result<String, ApiError> {
        let body = json!({"input": text.trim(), "source_language_code": bcp47(src), "target_language_code": "en-IN"});
        let data = self.post_json("/transliterate", &body).await?;
        let t = data.get("transliterated_text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
        if t.is_empty() { Err(ApiError::Other("The transliterator sent back nothing.".into())) } else { Ok(t) }
    }
}

const KEYTERMS: [&str; 6] = ["Sarvam", "Gemma", "DeepSeek", "prompt", "model", "Vak"];

#[async_trait]
impl Stt for Sarvam {
    async fn transcribe(&self, audio: Vec<u8>, mime: &str, lang: Option<&str>) -> Result<Transcript, ApiError> {
        let secs = wav::seconds(&audio).unwrap_or(audio.len() as f64 / 16000.0);
        if secs > MAX_CLIP_SECONDS {
            return Err(ApiError::Other("That recording is longer than 30 seconds. Try a shorter one.".into()));
        }
        if secs < 0.4 {
            return Err(ApiError::Other("I couldn't hear anything in that recording. Try again a little louder.".into()));
        }
        let code = lang.filter(|l| *l != "auto").map(bcp47).unwrap_or_else(|| "unknown".into());
        let mime = mime.to_string();
        let model = self.stt_model.clone();
        let resp = self
            .send("/speech-to-text", |rb| {
                let part = reqwest::multipart::Part::bytes(audio.clone()).file_name("clip.wav").mime_str(&mime).unwrap_or_else(|_| reqwest::multipart::Part::bytes(audio.clone()));
                let mut form = reqwest::multipart::Form::new().text("model", model.clone()).text("language_code", code.clone()).part("file", part);
                if model == "saaras:v4" {
                    form = form.text("keyterms", serde_json::to_string(&KEYTERMS).unwrap_or_default());
                }
                rb.multipart(form)
            })
            .await?;
        let body: Value = resp.json().await.map_err(|_| ApiError::Other("Sarvam sent back something unreadable.".into()))?;
        let spoken = body.get("language_code").and_then(|l| l.as_str()).and_then(Lang::get).map(|l| l.code.clone());
        Ok(Transcript { text: body.get("transcript").and_then(|t| t.as_str()).unwrap_or("").trim().to_string(), lang: spoken })
    }
}

#[async_trait]
impl Tts for Sarvam {
    async fn speak(&self, text: &str, lang: &str, gender: &str) -> Result<Vec<u8>, ApiError> {
        let l = Lang::get(lang).ok_or_else(|| ApiError::Other("Unknown language.".into()))?;
        let voices = l.tts.as_ref().ok_or_else(|| ApiError::Other(format!("{} has no voice yet.", l.english)))?;
        let text = lang::clean(text);
        if text.is_empty() {
            return Err(ApiError::Other("There is nothing to read aloud.".into()));
        }
        let speaker = if gender.eq_ignore_ascii_case("male") { &voices.male } else { &voices.female };
        let mut parts = vec![];
        for chunk in pieces(&text, TTS_CHUNK) {
            let mut body = json!({"text": chunk, "language_code": l.bcp47, "model": voices.model, "speaker": speaker, "pace": 1.0, "speech_sample_rate": 24000, "temperature": 0.45});
            let data = match self.post_json("/text-to-speech", &body).await {
                Err(ApiError::Other(_)) if *speaker != voices.female => {
                    body["speaker"] = json!(voices.female);
                    self.post_json("/text-to-speech", &body).await?
                }
                other => other?,
            };
            let audios = data.get("audios").and_then(|a| a.as_array()).cloned().unwrap_or_default();
            let b64 = audios.first().and_then(|a| a.as_str()).ok_or_else(|| ApiError::Other("No audio came back.".into()))?;
            parts.push(base64::engine::general_purpose::STANDARD.decode(b64).map_err(|_| ApiError::Other("The audio was unreadable.".into()))?);
        }
        Ok(wav::merge(parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_at_sentences() {
        let t = "ఒకటి. ".repeat(600);
        let p = pieces(&t, 1800);
        assert!(p.len() >= 2 && p.iter().all(|x| x.chars().count() <= 1800));
        assert_eq!(pieces("short", 100), vec!["short"]);
    }

    #[tokio::test]
    async fn limiter_spends_its_burst_then_waits() {
        let l = Limiter::new(600); // 10 a second, burst 300
        let t = Instant::now();
        for _ in 0..20 {
            l.acquire().await;
        }
        assert!(t.elapsed() < Duration::from_millis(200));
        let tight = Limiter::new(60);
        for _ in 0..30 {
            tight.acquire().await;
        }
        tight.penalize(0.2).await;
        let t = Instant::now();
        tight.acquire().await;
        assert!(t.elapsed() >= Duration::from_millis(150));
    }

    #[tokio::test]
    async fn no_key_is_an_auth_error_without_a_request() {
        let s = Sarvam::new("", "http://127.0.0.1:1", 60);
        assert!(matches!(s.chat(ChatRequest::new("sarvam-105b", vec![Msg::user("hi")])).await, Err(ApiError::Auth)));
        assert!(!s.has_key());
    }
}
