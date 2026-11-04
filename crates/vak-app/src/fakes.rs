//! Scripted stand-ins for the network, used by tests here and nowhere else.

use async_trait::async_trait;
use serde_json::json;

use crate::ports::*;

#[derive(Default)]
pub struct FakeLlm {
    pub fail: Option<ApiError>,
    /// Canned reply for the non-helper calls (explanations etc.), if a test wants one.
    pub reply: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl Llm for FakeLlm {
    async fn chat(&self, req: ChatRequest) -> Result<ChatReply, ApiError> {
        if let Some(e) = &self.fail {
            return Err(e.clone());
        }
        let system = req.messages.first().map(|m| m.content.clone()).unwrap_or_default();
        let user = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        let text = if system.starts_with("You rewrite a user's message") {
            json!({
                "paraphrases": [format!("{user} అని చెప్పండి"), format!("చెప్పండి, {user}"), format!("దయచేసి గమనించండి: {user}")],
                "opposites": [format!("{user} కాదు")],
            })
            .to_string()
        } else if system.starts_with("You grade answers") {
            let reference = user.split("Reference answer: <<<").nth(1).and_then(|r| r.split(">>>").next()).unwrap_or("").to_string();
            let letters: String = user
                .lines()
                .filter(|l| l.contains(". <<<"))
                .map(|l| {
                    let cand = l.split("<<<").nth(1).unwrap_or("").trim_end_matches(">>>");
                    if canon(cand) == canon(&reference) { 's' } else { 'd' }
                })
                .collect();
            json!({"v": letters, "why": {}}).to_string()
        } else if let Some(r) = self.reply.lock().unwrap().pop() {
            r
        } else {
            user.clone()
        };
        Ok(ChatReply { texts: vec![text; req.n as usize], tokens_in: 100, tokens_out: 20, cached: false })
    }
}

pub struct FakeTranslator;

#[async_trait]
impl Translator for FakeTranslator {
    async fn translate(&self, text: &str, _src: &str, dst: &str, _o: &TranslateOpts) -> Result<String, ApiError> {
        Ok(match dst {
            "hi" => "यह फ़िल्म बहुत अच्छी है, कहानी शानदार।".to_string(),
            "en" => "This film is very good, the story is wonderful.".to_string(),
            other => format!("[{other}] {text}"),
        })
    }
    async fn romanize(&self, _text: &str, _src: &str) -> Result<String, ApiError> {
        Ok("ee sinima chala bagundi, katha adbhutam.".into())
    }
}

/// The practice models answer with a label in the language of the question; the fake judge reads them as one meaning.
fn canon(label: &str) -> &str {
    match label {
        "సానుకూలం" | "सकारात्मक" | "positive" => "pos",
        "ప్రతికూలం" | "नकारात्मक" | "negative" => "neg",
        "మిశ్రమం" | "मिला-जुला" | "mixed" => "mix",
        other => other,
    }
}
