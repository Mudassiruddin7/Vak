//! Turns what the developer said or typed into a concrete plan. A language model reads the message only
//! when it contains spoken instructions; plain pasted text is used as it is, which saves a call. The model
//! is never trusted with the prompt itself: if the text it hands back isn't really in the message, the
//! quoted passage or the whole message is used instead.

use std::sync::Arc;

use serde_json::Value;
use vak_core::audit::{difficulty, AuditType, Effort, ALL_TYPES};
use vak_core::lang;
use vak_core::text::{extract_json, squash};

use crate::engine::Settings;
use crate::plan::{Plan, TargetSpec};
use crate::ports::{ApiError, ChatRequest, Llm, Msg};

const SYSTEM: &str = "You help a developer audit an AI model. They speak or type in Telugu, Hindi, English or a mix of these.\n\
Read their message and pull out:\n\
- \"prompt\": ONLY the question or text they want to send to the model, copied word for word from their message, in the same language and script. Leave out whatever they say to you about the audit, such as which model to test, \"test my model with this question\", \"check this\", or \"इस सवाल से परखो\". If they quoted the text, use the quote. If the message is nothing but the text to test, return all of it.\n\
- \"reference_output\": an answer the model gave that they want checked, copied exactly, or null.\n\
- \"target_hint\": the name of a model if they mention one (for example Gemma, DeepSeek, Sarvam), otherwise null. Model names may be spelt in Telugu or Devanagari letters.\n\
- \"audit_types\": only if they ask for particular kinds of checks, any of \"consistency\" (same answer when asked again), \"perturbation\" (small changes such as typos, rewording, other languages) and \"behavioral\" (sensible reaction when the meaning changes). General words like \"steady\", \"consistent\", \"reliable\" or \"check\" describe the goal, not a choice of checks, so use an empty list for them. Use an empty list whenever they did not name specific checks.\n\
- \"effort\": \"light\", \"balanced\" or \"thorough\" only if they said how thorough to be, otherwise null.\n\
Examples:\n\
Message: నా మోడల్\u{200c}ను ఈ ప్రశ్నతో పరీక్షించు. ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?\n\
{\"prompt\": \"ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?\", \"reference_output\": null, \"target_hint\": null, \"audit_types\": [], \"effort\": null}\n\
Message: जेम्मा मॉडल को इस सवाल से परखो दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?\n\
{\"prompt\": \"दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?\", \"reference_output\": null, \"target_hint\": \"Gemma\", \"audit_types\": [], \"effort\": null}\n\
Message: ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.\n\
{\"prompt\": \"ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.\", \"reference_output\": null, \"target_hint\": null, \"audit_types\": [], \"effort\": null}\n\
Reply with JSON only: {\"prompt\": \"\", \"reference_output\": null, \"target_hint\": null, \"audit_types\": [], \"effort\": null}";

const TARGET_KEYWORDS: [(&[&str], &str); 2] = [
    (&["gemma", "జెమ్మా", "జెమ్మ", "गेम्मा", "जेम्मा"], "gemma4"),
    (&["deepseek", "deep seek", "డీప్\u{200c}సీక్", "డీప్ సీక్", "डीपसीक", "डीप सीक"], "deepseekv4.1-flash-1017"),
];

/// Words that mean the person is talking to the tool rather than just pasting a prompt.
const INSTRUCTION_WORDS: &[&str] = &[
    "test", "check", "audit", "evaluate", "probe", "model", "try", "परख", "जांच", "जाँच", "टेस्ट", "मॉडल", "जांचो", "పరీక్ష", "చెక్", "తనిఖీ", "టెస్ట్", "మోడల్", "పరిశీలించు",
];

const WRAPS: [(char, char); 5] = [('"', '"'), ('“', '”'), ('‘', '’'), ('«', '»'), ('「', '」')];

/// Drop quotation marks wrapped around the whole text; the model sometimes copies them along.
pub fn unquote(text: &str) -> String {
    let mut t = text.trim().to_string();
    loop {
        let chars: Vec<char> = t.chars().collect();
        if chars.len() >= 2 && WRAPS.iter().any(|(o, c)| *o == chars[0] && *c == chars[chars.len() - 1]) {
            t = chars[1..chars.len() - 1].iter().collect::<String>().trim().to_string();
        } else {
            return t;
        }
    }
}

pub fn quoted_segments(text: &str) -> Vec<String> {
    let mut out = vec![];
    for (o, c) in WRAPS {
        let mut rest = text;
        while let Some(start) = rest.find(o) {
            let after = &rest[start + o.len_utf8()..];
            let Some(end) = after.find(c) else { break };
            let seg = after[..end].trim();
            if seg.chars().count() >= 4 {
                out.push(seg.to_string());
            }
            rest = &after[end + c.len_utf8()..];
        }
    }
    out
}

pub fn resolve_target(hint: Option<&str>, text: &str) -> Option<&'static str> {
    let hay = format!("{} {}", hint.unwrap_or(""), text).to_lowercase();
    TARGET_KEYWORDS.iter().find(|(keys, _)| keys.iter().any(|k| hay.contains(&k.to_lowercase()))).map(|(_, id)| *id)
}

fn talks_to_the_tool(text: &str) -> bool {
    let low = text.to_lowercase();
    INSTRUCTION_WORDS.iter().any(|w| low.contains(w)) || !quoted_segments(text).is_empty()
}

pub struct Orchestrator {
    pub llm: Arc<dyn Llm>,
    pub settings: Settings,
}

#[derive(Default, Clone)]
pub struct Overrides {
    pub effort: Option<Effort>,
    pub audit_types: Option<Vec<AuditType>>,
    pub target: Option<TargetSpec>,
    pub reference_output: Option<String>,
}

impl Orchestrator {
    pub async fn plan(&self, text: &str, overrides: &Overrides) -> Result<Plan, ApiError> {
        let text = text.trim();
        let mut notes: Vec<String> = vec![];
        let mut data: Option<Value> = None;
        if talks_to_the_tool(text) {
            match self.extract(text).await {
                Ok(d) => data = d,
                Err(e) if e.is_fatal() => return Err(e),
                Err(_) => notes.push("I couldn't use the language model to read your message, so I'm testing it as you wrote it.".into()),
            }
        }
        let mut prompt: Option<String> = None;
        if let Some(cand) = data.as_ref().and_then(|d| d.get("prompt")).and_then(|p| p.as_str()).map(unquote).filter(|c| !c.is_empty()) {
            if squash(text).contains(&squash(&cand)) {
                prompt = Some(cand);
            } else {
                notes.push("The text I pulled out didn't match what you said, so I went back to your own words.".into());
            }
        }
        let prompt = prompt.unwrap_or_else(|| {
            let quotes = quoted_segments(text);
            match quotes.into_iter().max_by_key(|q| q.chars().count()) {
                Some(q) => q,
                None => {
                    if data.is_some() && !notes.iter().any(|n| n.contains("your own words")) {
                        notes.push("I couldn't spot a separate question, so I'll test your whole message.".into());
                    }
                    text.to_string()
                }
            }
        });
        let limit = self.settings.max_prompt_chars;
        let prompt = if prompt.chars().count() > limit {
            notes.push(format!("That was long, so I kept the first {limit} characters."));
            let cut: String = prompt.chars().take(limit).collect();
            cut.rsplit_once(' ').map(|(a, _)| a.to_string()).unwrap_or(cut).trim().to_string()
        } else {
            prompt
        };
        let reference = data.as_ref().and_then(|d| d.get("reference_output")).and_then(|r| r.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let hint = data.as_ref().and_then(|d| d.get("target_hint")).and_then(|h| h.as_str());
        let target = match resolve_target(hint, text) {
            Some(m) => TargetSpec { model: m.into(), ..Default::default() },
            None => TargetSpec { model: self.settings.target_model.clone(), ..Default::default() },
        };
        let types: Vec<AuditType> = data
            .as_ref()
            .and_then(|d| d.get("audit_types"))
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|t| t.as_str()).filter_map(|t| ALL_TYPES.iter().find(|x| x.as_str() == t).copied()).collect())
            .unwrap_or_default();
        let said_effort = data.as_ref().and_then(|d| d.get("effort")).and_then(|e| e.as_str()).and_then(Effort::parse);
        let language = lang::detect(&prompt).map(|l| l.code.clone()).unwrap_or_else(|| "en".into());
        let mut plan = Plan {
            effort: said_effort.unwrap_or_else(|| difficulty(&prompt).suggested),
            prompt,
            reference_output: reference,
            target,
            audit_types: if types.is_empty() { ALL_TYPES.to_vec() } else { types },
            prompt_language: language,
            notes,
            source_text: text.to_string(),
        };
        if let Some(e) = overrides.effort {
            plan.effort = e;
        }
        if let Some(t) = overrides.audit_types.as_ref().filter(|t| !t.is_empty()) {
            plan.audit_types = t.clone();
        }
        if let Some(t) = &overrides.target {
            plan.target = t.clone();
        }
        if let Some(r) = overrides.reference_output.as_ref().filter(|r| !r.is_empty()) {
            plan.reference_output = Some(r.clone());
        }
        Ok(plan)
    }

    async fn extract(&self, text: &str) -> Result<Option<Value>, ApiError> {
        let req = ChatRequest::new(&self.settings.plan_model, vec![Msg::system(SYSTEM), Msg::user(text)]).json().max_tokens(500);
        let reply = self.llm.chat(req).await?;
        Ok(extract_json(reply.text()).filter(|v| v.is_object()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::FakeLlm;

    fn orch(reply: Option<&str>) -> Orchestrator {
        let llm = Arc::new(FakeLlm::default());
        if let Some(r) = reply {
            llm.reply.lock().unwrap().push(r.into());
        }
        Orchestrator { llm, settings: Settings::default() }
    }

    #[tokio::test]
    async fn plain_text_needs_no_model_call() {
        let o = orch(None);
        let plan = o.plan("ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.", &Overrides::default()).await.unwrap();
        assert_eq!(plan.prompt, "ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.");
        assert_eq!(plan.prompt_language, "te");
        assert_eq!(plan.effort, Effort::Light, "an easy prompt gets a light audit");
    }

    #[tokio::test]
    async fn spoken_instructions_are_stripped_by_the_model_but_checked() {
        let said = "జెమ్మా మోడల్‌ను ఈ ప్రశ్నతో పరీక్షించు. ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?";
        let good = r#"{"prompt": "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?", "reference_output": null, "target_hint": "Gemma", "audit_types": [], "effort": null}"#;
        let plan = orch(Some(good)).plan(said, &Overrides::default()).await.unwrap();
        assert!(plan.prompt.starts_with("ఒక కిలో") && plan.target.model == "gemma4", "{plan:?}");
        // an invented prompt is rejected in favour of the person's own words
        let fake = r#"{"prompt": "something the model made up entirely", "target_hint": null}"#;
        let plan = orch(Some(fake)).plan(said, &Overrides::default()).await.unwrap();
        assert_eq!(plan.prompt, said);
        assert!(plan.notes.iter().any(|n| n.contains("your own words")));
    }

    #[tokio::test]
    async fn quotes_overrides_and_length() {
        let plan = orch(Some("not json")).plan("check this: “दिल्ली से हैदराबाद कितनी दूर है?” please", &Overrides { effort: Some(Effort::Thorough), ..Default::default() }).await.unwrap();
        assert_eq!(plan.prompt, "दिल्ली से हैदराबाद कितनी दूर है?");
        assert_eq!(plan.effort, Effort::Thorough);
        let long = "పదం ".repeat(400);
        assert!(orch(None).plan(&long, &Overrides::default()).await.unwrap().prompt.chars().count() <= 600);
        assert_eq!(unquote("“ “hi there” ”"), "hi there");
    }
}
