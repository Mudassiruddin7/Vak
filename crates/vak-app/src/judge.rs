//! Deciding whether two answers say the same thing. A cheap lexical check settles the obvious cases in
//! the engine; everything else comes here in batches, with a very short reply format to save output tokens.

use std::collections::HashMap;
use std::sync::Arc;

use vak_core::text::{clip, extract_json, Verdict};

use crate::ports::{ApiError, ChatRequest, ChatReply, Llm, Msg};

const SYSTEM: &str = "You grade answers from an AI model. You get a reference answer and numbered candidates that came from lightly edited versions of the same task, so judge the answers only.\n\
Per candidate give one letter:\n\
s = same final answer, decision or meaning as the reference (wording, language, politeness and length do not matter)\n\
p = mostly the same, but a material detail differs: a number, condition, step or caveat\n\
d = a different final answer, a contradiction, a refusal where the other answered, or unrelated content\n\
Candidates may be in another language than the reference: compare meaning, not wording.\n\
Reply with JSON only: {\"v\":\"spd\",\"why\":{\"2\":\"at most 8 words\"}} where v has one letter per candidate in order, and why gives a reason only for p and d.";

pub struct Judge {
    pub llm: Arc<dyn Llm>,
    pub model: String,
    pub reasoning: Option<String>,
    pub batch_size: usize,
}

pub struct Judged {
    pub verdicts: HashMap<String, (Verdict, String)>,
    pub usage: Vec<ChatReply>,
}

impl Judge {
    pub async fn compare(&self, task: &str, reference: &str, candidates: &[(String, String)]) -> Result<Judged, ApiError> {
        let batches: Vec<&[(String, String)]> = candidates.chunks(self.batch_size).collect();
        let results = futures::future::join_all(batches.iter().map(|b| self.batch(task, reference, b))).await;
        let mut out = Judged { verdicts: HashMap::new(), usage: vec![] };
        for r in results {
            match r {
                Ok((map, reply)) => {
                    out.verdicts.extend(map);
                    out.usage.push(reply);
                }
                Err(e) if e.is_fatal() => return Err(e),
                Err(_) => {}
            }
        }
        Ok(out)
    }

    async fn batch(&self, task: &str, reference: &str, cands: &[(String, String)]) -> Result<(HashMap<String, (Verdict, String)>, ChatReply), ApiError> {
        let lines: Vec<String> = cands.iter().enumerate().map(|(i, (_, t))| format!("{}. <<<{}>>>", i + 1, clip(t, 450))).collect();
        let user = format!("Task: <<<{}>>>\nReference answer: <<<{}>>>\n\nCandidates:\n{}", clip(task, 400), clip(reference, 600), lines.join("\n"));
        let think = if self.reasoning.is_some() { 600 } else { 0 };
        let req = ChatRequest::new(&self.model, vec![Msg::system(SYSTEM), Msg::user(user)])
            .json()
            .max_tokens(40 + 24 * cands.len() as u32 + think)
            .reasoning(self.reasoning.as_deref());
        let reply = self.llm.chat(req).await?;
        Ok((parse(reply.text(), cands), reply))
    }
}

fn parse(text: &str, cands: &[(String, String)]) -> HashMap<String, (Verdict, String)> {
    let mut out = HashMap::new();
    let Some(data) = extract_json(text) else { return out };
    let letters: Vec<char> = data.get("v").and_then(|v| v.as_str()).unwrap_or("").chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let why = data.get("why");
    for (i, (key, _)) in cands.iter().enumerate() {
        let verdict = match letters.get(i).map(|c| c.to_ascii_lowercase()) {
            Some('s') => Verdict::Same,
            Some('p') => Verdict::Partly,
            Some('d') => Verdict::Different,
            _ => continue,
        };
        let reason = why.and_then(|w| w.get((i + 1).to_string())).and_then(|r| r.as_str()).unwrap_or("");
        out.insert(key.clone(), (verdict, clip(reason, 140)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_replies_parse() {
        let c: Vec<(String, String)> = (1..=3).map(|i| (format!("t{i}"), String::new())).collect();
        let m = parse(r#"{"v":"spd","why":{"2":"price differs","3":"unrelated"}}"#, &c);
        assert_eq!(m["t1"].0, Verdict::Same);
        assert_eq!(m["t2"], (Verdict::Partly, "price differs".into()));
        assert_eq!(m["t3"].0, Verdict::Different);
        // a short reply only covers what it covers; junk covers nothing
        assert_eq!(parse(r#"{"v":"s"}"#, &c).len(), 1);
        assert!(parse("sorry", &c).is_empty());
        let odd = parse(r#"{"v":"s x d"}"#, &c);
        assert!(!odd.contains_key("t2") && odd["t3"].0 == Verdict::Different, "an unknown letter leaves its slot empty");
    }
}
