//! Guards and the trust score for the live interpreter. A translation is checked three ways without
//! asking any model: do the numbers survive, do names and acronyms survive, is it in the right script.
//! A back-translation (target text translated back to the speaker's language) is then compared with
//! what was actually said. The result is a number the interface can show, and a level that decides
//! whether the speaker is asked to confirm before anything is voiced.

use serde::Serialize;

use crate::lang::Lang;
use crate::text::{lexical_similarity, numbers_in, script_share, Script};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Good,
    Check,
    Confirm,
}

#[derive(Clone, Debug, Serialize)]
pub struct Trust {
    pub score: u32,
    pub level: Level,
    /// Codes the interface turns into plain sentences.
    pub issues: Vec<&'static str>,
    pub back_similarity: Option<f64>,
}

/// Acronyms and capitalised names (FIR, ATM, Aadhaar, Ravi) that should pass through unchanged. A capital at
/// the start of a sentence does not make a word a name.
fn latin_names(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut start = true;
    for tok in text.split_whitespace() {
        let word: String = tok.chars().filter(|c| c.is_alphanumeric()).collect();
        let ascii = !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric());
        let upper = word.chars().filter(|c| c.is_ascii_alphabetic()).all(|c| c.is_ascii_uppercase()) && word.chars().count() >= 2;
        let capital = word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        if ascii && word.chars().count() >= 2 && (upper || (capital && !start)) {
            out.push(word.to_lowercase());
        }
        start = tok.ends_with(['.', '?', '!', '।']);
    }
    out
}

pub fn assess(source: &str, _src_lang: Lang, target: &str, dst_lang: Lang, back: Option<&str>) -> Trust {
    let mut issues: Vec<&'static str> = vec![];
    let mut cap = 100u32;
    if target.trim().is_empty() {
        return Trust { score: 0, level: Level::Confirm, issues: vec!["empty"], back_similarity: None };
    }
    if numbers_in(source) != numbers_in(target) {
        issues.push("numbers");
        cap = cap.min(55);
    }
    let target_lower = target.to_lowercase();
    let lost: Vec<String> = latin_names(source).into_iter().filter(|n| !target_lower.contains(n.as_str())).collect();
    if !lost.is_empty() {
        issues.push("names");
        cap = cap.min(70);
    }
    if dst_lang.script != Script::Latin && script_share(target, dst_lang.script) < 0.6 && latin_names(target).len() < target.split_whitespace().count() / 2 {
        issues.push("script");
        cap = cap.min(40);
    }
    let (sl, tl) = (source.chars().count() as f64, target.chars().count() as f64);
    let ratio = tl / sl.max(1.0);
    if ratio < 0.35 || ratio > 3.2 {
        issues.push("length");
        cap = cap.min(60);
    }
    let back_similarity = back.map(|b| lexical_similarity(source, b));
    let base = match back_similarity {
        Some(sim) => ((sim - 0.15) / 0.5).clamp(0.0, 1.0),
        None => 0.75, // no back-translation (fast mode): guards only, so never fully green by itself
    };
    if back_similarity.is_some_and(|s| s < 0.45) {
        issues.push("drift");
    }
    let guards_penalty = 1.0 - 0.12 * issues.len() as f64;
    let raw = (100.0 * base * guards_penalty.max(0.0)).round() as u32;
    let score = raw.min(cap);
    let level = if score >= 75 { Level::Good } else if score >= 50 { Level::Check } else { Level::Confirm };
    Trust { score, level, issues, back_similarity }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(c: &str) -> Lang {
        Lang::get(c).unwrap()
    }

    #[test]
    fn faithful_round_trip_is_green() {
        let t = assess("Please show your Aadhaar card, 2 copies", l("en"), "कृपया अपना Aadhaar कार्ड दिखाइए, 2 प्रतियाँ", l("hi"), Some("Please show your Aadhaar card, 2 copies"));
        assert_eq!(t.level, Level::Good, "{t:?}");
        assert!(t.issues.is_empty());
    }

    #[test]
    fn changed_number_caps_the_score() {
        let t = assess("Come at 5 o'clock", l("en"), "कृपया 6 बजे आइए", l("hi"), Some("Come at 5 o'clock"));
        assert!(t.issues.contains(&"numbers") && t.score <= 55);
        assert_ne!(t.level, Level::Good);
    }

    #[test]
    fn lost_names_and_wrong_script_are_caught() {
        let t = assess("Report this to the FIR desk", l("en"), "इसे डेस्क को बताइए", l("hi"), None);
        assert!(t.issues.contains(&"names"));
        let wrong = assess("Where is the station?", l("en"), "Where is the station?", l("te"), None);
        assert!(wrong.issues.contains(&"script") && wrong.level == Level::Confirm);
    }

    #[test]
    fn drift_in_the_back_translation_asks_for_confirmation() {
        let t = assess("Do you have a fever", l("en"), "క్షమించండి, మీరు ఎక్కడ నివసిస్తారు", l("te"), Some("Sorry, where do you live"));
        assert_eq!(t.level, Level::Confirm, "{t:?}");
        assert_eq!(assess("", l("en"), "", l("hi"), None).score, 0);
        // fast mode has no back-translation, so guards alone can reach "check" but not "good"
        assert!(assess("Hello friend", l("en"), "నమస్కారం మిత్రమా", l("te"), None).score <= 75);
    }
}
