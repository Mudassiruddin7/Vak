use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum ApiError {
    #[error("the API key was rejected")]
    Auth,
    #[error("the account is out of credits")]
    OutOfCredits,
    #[error("timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

impl ApiError {
    /// Auth and credit problems end the whole run; anything else only costs one test.
    pub fn is_fatal(&self) -> bool {
        matches!(self, ApiError::Auth | ApiError::OutOfCredits)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

impl Msg {
    pub fn system(c: impl Into<String>) -> Msg {
        Msg { role: "system".into(), content: c.into() }
    }
    pub fn user(c: impl Into<String>) -> Msg {
        Msg { role: "user".into(), content: c.into() }
    }
    pub fn assistant(c: impl Into<String>) -> Msg {
        Msg { role: "assistant".into(), content: c.into() }
    }
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Msg>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub n: u32,
    pub json: bool,
    /// `Some("low")` turns thinking on for models that support it; `None` keeps it off.
    pub reasoning: Option<String>,
    /// Allow an answer cache to serve this (only ever for temperature 0).
    pub cache: bool,
}

impl ChatRequest {
    pub fn new(model: &str, messages: Vec<Msg>) -> Self {
        ChatRequest { model: model.into(), messages, temperature: 0.0, max_tokens: 400, n: 1, json: false, reasoning: None, cache: true }
    }
    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self.cache = self.cache && t == 0.0;
        self
    }
    pub fn n(mut self, n: u32) -> Self {
        self.n = n;
        self
    }
    pub fn json(mut self) -> Self {
        self.json = true;
        self
    }
    pub fn reasoning(mut self, r: Option<&str>) -> Self {
        self.reasoning = r.map(String::from);
        self
    }
    pub fn no_cache(mut self) -> Self {
        self.cache = false;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChatReply {
    pub texts: Vec<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cached: bool,
}

impl ChatReply {
    pub fn text(&self) -> &str {
        self.texts.first().map(|s| s.as_str()).unwrap_or("")
    }
}

#[async_trait]
pub trait Llm: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatReply, ApiError>;
}

#[derive(Clone, Debug, Default)]
pub struct TranslateOpts {
    /// "formal" | "modern-colloquial" | "classic-colloquial" | "code-mixed"
    pub mode: Option<String>,
    pub speaker_gender: Option<String>,
    pub native_numerals: bool,
}

#[async_trait]
pub trait Translator: Send + Sync {
    /// Codes are Vak's short codes ("te", "hi", "en").
    async fn translate(&self, text: &str, src: &str, dst: &str, opts: &TranslateOpts) -> Result<String, ApiError>;
    /// Native script to English letters, the way people type on a phone.
    async fn romanize(&self, text: &str, src: &str) -> Result<String, ApiError>;
}

#[derive(Clone, Debug)]
pub struct Transcript {
    pub text: String,
    /// Short code of the language that was spoken, when known.
    pub lang: Option<String>,
}

#[async_trait]
pub trait Stt: Send + Sync {
    async fn transcribe(&self, audio: Vec<u8>, mime: &str, lang: Option<&str>) -> Result<Transcript, ApiError>;
}

#[async_trait]
pub trait Tts: Send + Sync {
    /// WAV bytes.
    async fn speak(&self, text: &str, lang: &str, gender: &str) -> Result<Vec<u8>, ApiError>;
}

/// Receives progress events as (event name, JSON payload).
pub type Emit = tokio::sync::mpsc::UnboundedSender<(String, serde_json::Value)>;

pub fn emit(tx: &Emit, event: &str, data: serde_json::Value) {
    let _ = tx.send((event.to_string(), data));
}
