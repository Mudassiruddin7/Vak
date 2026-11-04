//! Setu, the live interpreter. One turn takes what a person said (already transcribed), translates it for
//! the other person, and says how far to trust the result. In careful mode the translation is also
//! translated back and compared with what was said, so a drifting translation is caught before it is voiced.

use std::sync::Arc;

use serde::Serialize;
use vak_core::lang::{Lang, Tier};
use vak_core::live::{assess, Level, Trust};

use crate::ports::{ApiError, TranslateOpts, Translator};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Domain {
    pub id: &'static str,
    pub label: &'static str,
    /// Translation register: "formal" or "modern-colloquial".
    pub register: &'static str,
    pub careful: bool,
    /// Whether to show and speak the consent notice before the first turn.
    pub consent: bool,
}

pub const DOMAINS: [Domain; 7] = [
    Domain { id: "general", label: "Everyday talk", register: "modern-colloquial", careful: false, consent: false },
    Domain { id: "public_service", label: "Police and public service", register: "formal", careful: true, consent: true },
    Domain { id: "health", label: "Clinic and hospital", register: "formal", careful: true, consent: true },
    Domain { id: "bank", label: "Bank and KYC", register: "formal", careful: true, consent: true },
    Domain { id: "legal_aid", label: "Legal aid and courts", register: "formal", careful: true, consent: true },
    Domain { id: "market", label: "Market and farm", register: "modern-colloquial", careful: false, consent: false },
    Domain { id: "travel", label: "Travel and directions", register: "modern-colloquial", careful: false, consent: false },
];

pub fn domain(id: &str) -> &'static Domain {
    DOMAINS.iter().find(|d| d.id == id).unwrap_or(&DOMAINS[0])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fast,
    Careful,
}

#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub text: String,
    pub src: String,
    pub dst: String,
    pub gender: Option<String>,
    pub mode: Mode,
    pub domain: String,
    pub native_numerals: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TurnResult {
    pub source: String,
    pub translation: String,
    pub back_translation: Option<String>,
    pub trust: Trust,
    pub needs_confirm: bool,
    /// Whether the target language has a Sarvam voice.
    pub can_speak: bool,
    pub chars_translated: usize,
}

pub struct Interpreter {
    pub translator: Arc<dyn Translator>,
}

pub const MAX_TURN_CHARS: usize = 900;

impl Interpreter {
    pub async fn turn(&self, req: &TurnRequest) -> Result<TurnResult, ApiError> {
        let (src, dst) = match (Lang::get(&req.src), Lang::get(&req.dst)) {
            (Some(s), Some(d)) if s != d => (s, d),
            _ => return Err(ApiError::Other("Pick two different languages.".into())),
        };
        let text: String = req.text.trim().chars().take(MAX_TURN_CHARS).collect();
        if text.is_empty() {
            return Err(ApiError::Other("Nothing was said.".into()));
        }
        let dom = domain(&req.domain);
        let opts = TranslateOpts { mode: Some(dom.register.into()), speaker_gender: req.gender.clone(), native_numerals: req.native_numerals };
        let translation = self.translator.translate(&text, &src.code, &dst.code, &opts).await?;
        let mut chars = text.chars().count();
        let mut back = None;
        if req.mode == Mode::Careful {
            // the way back is plain: no gender or numeral choices, just the meaning
            let b = self.translator.translate(&translation, &dst.code, &src.code, &TranslateOpts { mode: Some("formal".into()), ..Default::default() }).await;
            match b {
                Ok(b) => back = Some(b),
                Err(e) if e.is_fatal() => return Err(e),
                Err(_) => {}
            }
            chars += translation.chars().count();
        }
        let trust = assess(&text, src, &translation, dst, back.as_deref());
        Ok(TurnResult {
            needs_confirm: trust.level == Level::Confirm,
            source: text,
            translation,
            back_translation: back,
            trust,
            can_speak: dst.tier == Tier::Full,
            chars_translated: chars,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Scripted;

    #[async_trait]
    impl Translator for Scripted {
        async fn translate(&self, text: &str, _s: &str, dst: &str, o: &TranslateOpts) -> Result<String, ApiError> {
            Ok(match (dst, text) {
                ("hi", _) => format!("कृपया अपना कार्ड दिखाइए ({})", o.mode.clone().unwrap_or_default()),
                ("en", t) if t.starts_with("कृपया") => "Please show your card".to_string(),
                (_, t) => t.to_string(),
            })
        }
        async fn romanize(&self, t: &str, _s: &str) -> Result<String, ApiError> {
            Ok(t.into())
        }
    }

    fn req(mode: Mode, domain: &str) -> TurnRequest {
        TurnRequest { text: "Please show your card".into(), src: "en".into(), dst: "hi".into(), gender: Some("Female".into()), mode, domain: domain.into(), native_numerals: false }
    }

    #[tokio::test]
    async fn careful_mode_translates_back_and_uses_the_domain_register() {
        let i = Interpreter { translator: Arc::new(Scripted) };
        let r = i.turn(&req(Mode::Careful, "public_service")).await.unwrap();
        assert!(r.translation.contains("formal"), "{}", r.translation);
        assert_eq!(r.back_translation.as_deref(), Some("Please show your card"));
        assert_eq!(r.trust.level, Level::Good, "{:?}", r.trust);
        assert!(!r.needs_confirm && r.can_speak);
        let fast = i.turn(&req(Mode::Fast, "market")).await.unwrap();
        assert!(fast.translation.contains("modern-colloquial") && fast.back_translation.is_none());
        assert_ne!(fast.trust.level, Level::Good, "guards alone never reach green");
    }

    #[tokio::test]
    async fn bad_requests_are_refused() {
        let i = Interpreter { translator: Arc::new(Scripted) };
        let mut same = req(Mode::Fast, "general");
        same.dst = "en".into();
        assert!(i.turn(&same).await.is_err());
        let mut empty = req(Mode::Fast, "general");
        empty.text = "  ".into();
        assert!(i.turn(&empty).await.is_err());
        let mut text_only = req(Mode::Fast, "general");
        text_only.dst = "ur".into();
        assert!(!i.turn(&text_only).await.unwrap().can_speak);
        assert_eq!(domain("nope").id, "general");
    }
}
