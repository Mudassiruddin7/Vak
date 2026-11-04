//! The model being audited: Sarvam-hosted, a user's own endpoint (built in the adapters crate), or a
//! built-in practice model.

use std::sync::Arc;

use async_trait::async_trait;
use vak_core::audit::practice;

use crate::catalog::chat_model;
use crate::ports::{ApiError, ChatRequest, Llm, Msg};

#[derive(Clone, Debug)]
pub struct Answer {
    pub text: String,
    pub error: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cached: bool,
}

impl Answer {
    pub fn ok(text: String) -> Self {
        Answer { text, error: None, tokens_in: 0, tokens_out: 0, cached: false }
    }
    pub fn failed(e: impl Into<String>) -> Self {
        Answer { text: String::new(), error: Some(e.into()), tokens_in: 0, tokens_out: 0, cached: false }
    }
    pub fn usable(&self) -> bool {
        self.error.is_none() && !self.text.trim().is_empty()
    }
}

#[async_trait]
pub trait Target: Send + Sync {
    fn label(&self) -> String;
    /// Rule-based stand-ins cannot describe their own reasons.
    fn can_explain(&self) -> bool {
        true
    }
    /// Which model id the cost should be charged to, if any.
    fn billed_model(&self) -> Option<String> {
        None
    }
    async fn ask(&self, prompt: &str, temperature: f32, n: u32, max_tokens: u32) -> Result<Vec<Answer>, ApiError>;
}

pub struct SarvamTarget {
    pub llm: Arc<dyn Llm>,
    pub model: String,
    pub system: Option<String>,
    pub use_cache: bool,
}

#[async_trait]
impl Target for SarvamTarget {
    fn label(&self) -> String {
        chat_model(&self.model).map(|m| m.label.to_string()).unwrap_or_else(|| self.model.clone())
    }
    fn billed_model(&self) -> Option<String> {
        Some(self.model.clone())
    }
    async fn ask(&self, prompt: &str, temperature: f32, n: u32, max_tokens: u32) -> Result<Vec<Answer>, ApiError> {
        let mut messages = vec![];
        if let Some(s) = &self.system {
            messages.push(Msg::system(s.clone()));
        }
        messages.push(Msg::user(prompt));
        let mut req = ChatRequest::new(&self.model, messages).temperature(temperature).max_tokens(max_tokens).n(n);
        req.cache = req.cache && self.use_cache;
        let reply = self.llm.chat(req).await?;
        let per = (reply.tokens_in / n.max(1) as u64, reply.tokens_out / n.max(1) as u64);
        let mut answers: Vec<Answer> = reply
            .texts
            .iter()
            .map(|t| {
                let t = vak_core::text::strip_think(t);
                let mut a = if t.is_empty() { Answer::failed("The model sent back an empty answer.") } else { Answer::ok(t) };
                a.tokens_in = per.0;
                a.tokens_out = per.1;
                a.cached = reply.cached;
                a
            })
            .collect();
        while answers.len() < n as usize {
            answers.push(Answer::failed("The model returned fewer answers than asked for."));
        }
        Ok(answers)
    }
}

pub struct PracticeTarget {
    pub steady: bool,
}

#[async_trait]
impl Target for PracticeTarget {
    fn label(&self) -> String {
        practice::MODELS[if self.steady { 0 } else { 1 }].1.to_string()
    }
    fn can_explain(&self) -> bool {
        false
    }
    async fn ask(&self, prompt: &str, _t: f32, n: u32, _m: u32) -> Result<Vec<Answer>, ApiError> {
        Ok((0..n).map(|_| Answer::ok(practice::mood(prompt, self.steady).to_string())).collect())
    }
}

pub fn practice_target(id: &str) -> Option<PracticeTarget> {
    practice::MODELS.iter().any(|(k, _)| *k == id).then(|| PracticeTarget { steady: id == "mood-steady" })
}
