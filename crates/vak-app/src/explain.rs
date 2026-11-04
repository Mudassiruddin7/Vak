//! Asks a model for a plain-language write-up and holds it to the writing rules. If it can't comply after
//! a few tries, a prepared explanation takes over, so the person always gets something readable.

use std::sync::Arc;

use serde::Serialize;
use vak_core::audit::Report;
use vak_core::lang::{self, text_for, Facts, Lang};
use vak_core::text::split_sentences;

use crate::ports::{ApiError, ChatRequest, Llm, Msg};

#[derive(Clone, Debug, Serialize)]
pub struct Explanation {
    pub language: String,
    pub text: String,
    pub sentences: Vec<String>,
    /// "model" or "template"
    pub source: String,
    pub attempts: usize,
    pub model: String,
    pub notes: Vec<String>,
    /// True when the wording was machine-translated and no native speaker has reviewed it.
    pub unreviewed: bool,
}

pub struct Explainer {
    pub llm: Arc<dyn Llm>,
    pub model: String,
}

impl Explainer {
    pub async fn explain(&self, report: &Report, lang: Lang, seed: &str, max_attempts: usize, reasoning: Option<&str>) -> Result<Explanation, ApiError> {
        let mut messages = vec![Msg::system(lang::build_system(report, lang, seed)), Msg::user(lang::build_user(lang))];
        let facts = Facts::from_report(report);
        let mut best: Option<(String, lang::Violations)> = None;
        let mut notes = vec![];
        let mut attempts = 0;
        for n in 1..=max_attempts.max(1) {
            attempts = n;
            let req = ChatRequest::new(&self.model, messages.clone()).temperature(0.4).max_tokens(1500).no_cache().reasoning(reasoning);
            let reply = match self.llm.chat(req).await {
                Ok(r) => r,
                Err(e) if e.is_fatal() => return Err(e),
                Err(e) => {
                    notes.push(format!("The language model was unavailable ({}).", e.to_string().chars().take(80).collect::<String>()));
                    break;
                }
            };
            let text = lang::clean(&vak_core::text::strip_think(reply.text()));
            let found = lang::check(&text, lang, Some(&facts));
            if best.as_ref().is_none_or(|(_, b)| found.severity() < b.severity()) {
                best = Some((text.clone(), found.clone()));
            }
            if found.ok() {
                break;
            }
            messages.push(Msg::assistant(reply.text().to_string()));
            messages.push(Msg::user(found.feedback()));
        }
        let unreviewed = text_for(&lang.code).is_some_and(|t| t.machine_translated);
        if let Some((mut text, mut found)) = best {
            if !found.long_sentences.is_empty() {
                text = lang::split_long(&text, lang);
                found = lang::check(&text, lang, Some(&facts));
            }
            if found.too_long {
                text = split_sentences(&text).into_iter().take(lang::MAX_SENTENCES).collect::<Vec<_>>().join(" ");
                found = lang::check(&text, lang, Some(&facts));
            }
            if !found.has_hard() && !found.latin_heavy {
                if !found.long_sentences.is_empty() {
                    notes.push(format!("A few sentences are a little longer than {} words.", lang.max_words));
                }
                return Ok(self.make(lang, text, "model", attempts, notes, unreviewed));
            }
            notes.push("The model's wording kept breaking the writing rules, so a prepared explanation is shown.".into());
        }
        Ok(self.make(lang, lang::fallback_explanation(report, lang), "template", attempts, notes, unreviewed))
    }

    fn make(&self, lang: Lang, text: String, source: &str, attempts: usize, notes: Vec<String>, unreviewed: bool) -> Explanation {
        Explanation {
            language: lang.code.clone(),
            sentences: split_sentences(&text),
            text,
            source: source.into(),
            attempts,
            model: self.model.clone(),
            notes,
            unreviewed: unreviewed && source == "template",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::FakeLlm;
    use vak_core::audit::*;

    pub fn report() -> Report {
        Report {
            prompt: "ఈ సినిమా బాగుంది".into(),
            prompt_language: "te".into(),
            baseline_output: "సానుకూలం".into(),
            reference_output: None,
            target_label: "Mood Meter (shaky)".into(),
            depth: "standard".into(),
            effort: "balanced".into(),
            audit_types: ALL_TYPES.to_vec(),
            consistency_score: 0.64,
            grade: Grade::Mixed,
            total_perturbations: 14,
            unexpected_changes: 5,
            skipped: 0,
            tests: vec![],
            groups: vec![],
            units: vec![],
            faithfulness: Faithfulness::skipped(""),
            flags: vec![Flag { code: "surface_noise".into(), severity: Severity::Medium, title: "t".into(), detail: "d".into(), evidence: vec![] }],
            strengths: vec![],
            next_steps: vec![],
            next_step_codes: vec![],
            not_tested: vec![],
            not_tested_codes: vec![],
            notes: vec![],
            judge_used: true,
            cost_inr: 0.0,
            seconds: 0.0,
            calls: CallStats::default(),
        }
    }

    #[tokio::test]
    async fn a_compliant_reply_is_used_and_a_bad_one_falls_back() {
        let good = "మోడల్ అంటే ప్రశ్నలకు జవాబులు ఇచ్చే కంప్యూటర్ భాగం. మేము మీ ప్రశ్నను చిన్న మార్పులతో 14 సార్లు అడిగాము. వాటిలో 5 సార్లు జవాబు మారింది. స్థిరత్వానికి 100 కి 64 మార్కులు వచ్చాయి. ఇది కొంచెం తడబడుతోంది. వాడే ముందు జాగ్రత్త తీసుకోండి.";
        let llm = Arc::new(FakeLlm::default());
        llm.reply.lock().unwrap().push(good.to_string());
        let ex = Explainer { llm: llm.clone(), model: "m".into() };
        let r = ex.explain(&report(), Lang::get("te").unwrap(), "s", 3, None).await.unwrap();
        assert_eq!((r.source.as_str(), r.attempts), ("model", 1), "{:?}", r.notes);

        let llm = Arc::new(FakeLlm::default());
        for _ in 0..3 {
            llm.reply.lock().unwrap().push("This is English and uses a token.".to_string());
        }
        let r = Explainer { llm, model: "m".into() }.explain(&report(), Lang::get("te").unwrap(), "s", 3, None).await.unwrap();
        assert_eq!((r.source.as_str(), r.attempts), ("template", 3));
        assert!(vak_core::text::script_share(&r.text, vak_core::text::Script::Telugu) > 0.9);
        assert!(!r.unreviewed, "Telugu wording is hand-written");
        let ta = Explainer { llm: Arc::new(FakeLlm::default()), model: "m".into() }.explain(&report(), Lang::get("ta").unwrap(), "s", 1, None).await.unwrap();
        assert!(ta.unreviewed && ta.source == "template");
    }
}
