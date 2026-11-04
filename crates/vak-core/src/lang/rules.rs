//! The writing rules from the brief, enforced in code instead of hoped for: banned words, a sentence
//! length limit that depends on the language, the right script, plain text only, and no invented numbers.
//! The model gets its own violations fed back; whatever survives is repaired or replaced by a template.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use super::texts::{text_for, ENGLISH_JARGON};
use super::Lang;
use crate::audit::Report;
use crate::text::{numbers_in, script_counts, script_share, split_sentences, word_count, Script};

pub const MIN_SENTENCES: usize = 4;
pub const MAX_SENTENCES: usize = 14;

/// "3 steps", "2 times" are fine even when the audit never mentions them.
const SMALL_NUMBERS: std::ops::RangeInclusive<u32> = 0..=10;

static MARKUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[*_#`>~|]+").expect("static regex"));
static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*(?:[-•●▪]|\d+[.)])\s+").expect("static regex"));
static EMOJI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}\u{1F000}-\u{1F2FF}\u{FE0F}]").expect("static regex"));
static JOINERS: LazyLock<Regex> = LazyLock::new(|| Regex::new("[\u{200B}-\u{200D}\u{2060}\u{FEFF}]").expect("static regex"));
static GAPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("static regex"));
static NEWLINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\n\s*").expect("static regex"));

/// Text with joiners removed and case folded, for substring checks.
fn plain(text: &str) -> String {
    JOINERS.replace_all(&text.nfkc().collect::<String>(), "").to_lowercase()
}

/// Strip markup, bullets and emoji; fold the reply into one tidy paragraph.
pub fn clean(text: &str) -> String {
    let t = EMOJI.replace_all(text, "");
    let t = BULLET.replace_all(&t, "");
    let t = MARKUP.replace_all(&t, "");
    let t = NEWLINES.replace_all(&t, " ");
    GAPS.replace_all(&t, " ").trim().to_string()
}

/// Terms that must not appear, English and in this language's spelling. Substring match, so plurals are covered.
fn banned_terms(lang: Lang) -> Vec<String> {
    let mut terms: Vec<String> = ENGLISH_JARGON.iter().map(|s| s.to_string()).collect();
    if let Some(t) = text_for(&lang.code) {
        terms.extend(t.jargon.iter().cloned());
    }
    // Marathi and Hindi share Devanagari, so each also rules out the other's spellings
    let sibling = match lang.code.as_str() {
        "mr" => Some("hi"),
        "hi" => Some("mr"),
        _ => None,
    };
    if let Some(t) = sibling.and_then(text_for) {
        terms.extend(t.jargon.iter().cloned());
    }
    terms
}

/// What the explanation is allowed to say numerically. Anything else counts as made up.
#[derive(Clone, Debug)]
pub struct Facts {
    pub score_100: i64,
    pub allowed: HashSet<String>,
}

impl Facts {
    pub fn from_report(report: &Report) -> Facts {
        let score = report.consistency_score;
        let mut nums: HashSet<String> = HashSet::new();
        for n in [
            format!("{}", (score * 100.0).round() as i64),
            format!("{score:.2}"),
            format!("{score}"),
            "100".to_string(),
            report.total_perturbations.to_string(),
            report.unexpected_changes.to_string(),
            report.skipped.to_string(),
        ] {
            nums.insert(n);
        }
        let mut texts: Vec<&str> = vec![&report.target_label, &report.prompt, &report.baseline_output];
        texts.extend(report.not_tested.iter().map(|s| s.as_str()));
        texts.extend(report.next_steps.iter().map(|s| s.as_str()));
        texts.extend(report.strengths.iter().map(|s| s.as_str()));
        texts.extend(report.flags.iter().map(|f| f.detail.as_str()));
        for t in texts {
            nums.extend(numbers_in(t));
        }
        nums.extend(SMALL_NUMBERS.map(|n| n.to_string()));
        Facts { score_100: (score * 100.0).round() as i64, allowed: nums }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Violations {
    pub banned: Vec<String>,
    pub long_sentences: Vec<String>,
    pub wrong_script: bool,
    pub latin_heavy: bool,
    pub sentence_count: usize,
    pub too_short: bool,
    pub too_long: bool,
    pub invented_numbers: Vec<String>,
    pub missing_score: bool,
    pub max_words: usize,
}

impl Violations {
    pub fn ok(&self) -> bool {
        !(self.has_hard() || !self.long_sentences.is_empty() || self.latin_heavy || self.too_long || self.missing_score)
    }

    /// Problems that make a draft unusable, as opposed to merely untidy.
    pub fn has_hard(&self) -> bool {
        !self.banned.is_empty() || self.wrong_script || self.too_short || !self.invented_numbers.is_empty()
    }

    /// Lower is better; used to pick the best of several attempts.
    pub fn severity(&self) -> (u32, usize, u32, usize) {
        (
            (!self.banned.is_empty()) as u32 + self.wrong_script as u32 + self.too_short as u32 + (!self.invented_numbers.is_empty()) as u32,
            self.banned.len() + self.invented_numbers.len(),
            self.latin_heavy as u32 + self.too_long as u32 + self.missing_score as u32,
            self.long_sentences.len(),
        )
    }

    pub fn feedback(&self) -> String {
        let mut lines: Vec<String> = vec![];
        if !self.banned.is_empty() {
            lines.push(format!(
                "You used words that are not allowed: {}. Describe the idea in plain everyday words instead.",
                self.banned.join(", ")
            ));
        }
        if !self.long_sentences.is_empty() {
            let shown: Vec<String> = self
                .long_sentences
                .iter()
                .take(3)
                .map(|s| if s.chars().count() < 120 { s.clone() } else { format!("{}...", s.chars().take(117).collect::<String>()) })
                .collect();
            lines.push(format!("These sentences are longer than {} words. Split or shorten them: {}", self.max_words, shown.join(" / ")));
        }
        if self.wrong_script || self.latin_heavy {
            lines.push("Write only in the requested script. Do not use English letters or English words (digits are fine).".into());
        }
        if self.too_short {
            lines.push("The explanation is too short. Write 8 to 12 sentences covering what was found, what it means and what to do next.".into());
        }
        if self.too_long {
            lines.push("The explanation is too long. Keep it to 12 sentences.".into());
        }
        if !self.invented_numbers.is_empty() {
            lines.push(format!("You used numbers that are not in the audit: {}. Use only the numbers you were given.", self.invented_numbers.join(", ")));
        }
        if self.missing_score {
            lines.push("Say the consistency score as a number out of 100, written in digits.".into());
        }
        lines.join("\n")
    }
}

pub fn check(text: &str, lang: Lang, facts: Option<&Facts>) -> Violations {
    let folded = plain(text);
    let sentences = split_sentences(text);
    let mut v = Violations { sentence_count: sentences.len(), max_words: lang.max_words, ..Default::default() };
    let mut banned: Vec<String> = banned_terms(lang).into_iter().filter(|t| folded.contains(&plain(t))).collect();
    banned.sort();
    banned.dedup();
    v.banned = banned;
    v.long_sentences = sentences.iter().filter(|s| word_count(s) > lang.max_words).cloned().collect();
    let counts = script_counts(text);
    let total: usize = counts.values().sum();
    v.wrong_script = total == 0 || script_share(text, lang.script) < 0.6;
    if lang.script != Script::Latin {
        v.latin_heavy = total > 0 && *counts.get(&Script::Latin).unwrap_or(&0) as f64 / total as f64 > 0.08;
    }
    v.too_short = sentences.len() < MIN_SENTENCES;
    v.too_long = sentences.len() > MAX_SENTENCES;
    if let Some(f) = facts {
        let used = numbers_in(text);
        v.invented_numbers = used.iter().filter(|n| !f.allowed.contains(*n)).cloned().collect();
        v.missing_score = !used.contains(&f.score_100.to_string());
    }
    v
}

/// Words a sentence can be cut in front of, by language. Others cut on commas only.
fn split_words(code: &str) -> &'static [&'static str] {
    match code {
        "te" => &["కానీ", "అయితే", "మరియు", "కాబట్టి", "అందువల్ల", "లేదా"],
        "hi" => &["लेकिन", "मगर", "और", "इसलिए", "या", "जबकि"],
        "mr" => &["पण", "आणि", "म्हणून", "किंवा"],
        "ta" => &["ஆனால்", "மற்றும்", "எனவே", "அல்லது"],
        "kn" => &["ಆದರೆ", "ಮತ್ತು", "ಆದ್ದರಿಂದ", "ಅಥವಾ"],
        "ml" => &["പക്ഷേ", "കൂടാതെ", "അതിനാൽ", "അല്ലെങ്കിൽ"],
        "bn" => &["কিন্তু", "এবং", "তাই", "অথবা"],
        "gu" => &["પણ", "અને", "તેથી", "અથવા"],
        "pa" => &["ਪਰ", "ਅਤੇ", "ਇਸ ਲਈ", "ਜਾਂ"],
        "od" => &["କିନ୍ତୁ", "ଏବଂ", "ତେଣୁ", "କିମ୍ବା"],
        _ => &["but", "and", "so", "or"],
    }
}

/// Last-resort repair: cut over-long sentences at a comma or conjunction near the middle.
pub fn split_long(text: &str, lang: Lang) -> String {
    let limit = lang.max_words;
    split_sentences(text).iter().flat_map(|s| cut(s, lang, limit)).collect::<Vec<_>>().join(" ")
}

fn cut(sentence: &str, lang: Lang, limit: usize) -> Vec<String> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if words.len() <= limit {
        return vec![sentence.to_string()];
    }
    let stop = lang.sentence_end.as_str();
    let end = match sentence.chars().last() {
        Some(c) if ".?!।॥".contains(c) => c.to_string(),
        _ => stop.to_string(),
    };
    let body = sentence.trim_end_matches(|c| ".?!।॥".contains(c));
    let words: Vec<&str> = body.split_whitespace().collect();
    let joiners = split_words(&lang.code);
    let mut candidates = vec![];
    for i in 0..words.len().saturating_sub(1) {
        let at = i + 1;
        if (3..=words.len().saturating_sub(3)).contains(&at) && (words[i].ends_with([',', '،', ';']) || joiners.contains(&words[i + 1])) {
            candidates.push(at);
        }
    }
    let Some(&split) = candidates.iter().min_by(|a, b| {
        let mid = words.len() as f64 / 2.0;
        ((**a as f64 - mid).abs()).partial_cmp(&((**b as f64 - mid).abs())).unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return vec![sentence.to_string()];
    };
    let first = words[..split].join(" ");
    let first = first.trim_end_matches([',', ';', '،']).to_string();
    let rest = words[split..].join(" ");
    let mut out = cut(&format!("{first}{stop}"), lang, limit);
    out.extend(cut(&format!("{rest}{end}"), lang, limit));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn te() -> Lang {
        Lang::get("te").unwrap()
    }
    fn hi() -> Lang {
        Lang::get("hi").unwrap()
    }

    fn facts(score: i64, extra: &[&str]) -> Facts {
        let mut allowed: HashSet<String> = (0..=10).map(|n| n.to_string()).collect();
        allowed.insert(score.to_string());
        allowed.insert("100".into());
        allowed.extend(extra.iter().map(|s| s.to_string()));
        Facts { score_100: score, allowed }
    }

    const GOOD_TE: &str = "మోడల్ అంటే ప్రశ్నలకు జవాబులు ఇచ్చే కంప్యూటర్ భాగం. మేము మీ ప్రశ్నను చిన్న మార్పులతో 14 సార్లు అడిగాము. వాటిలో 5 సార్లు జవాబు మారింది. స్థిరత్వానికి 100 కి 64 మార్కులు వచ్చాయి. ఇది కొంచెం తడబడుతోంది. వాడే ముందు జాగ్రత్త తీసుకోండి.";

    #[test]
    fn clean_text_passes() {
        let v = check(GOOD_TE, te(), Some(&facts(64, &["14", "5"])));
        assert!(v.ok(), "{v:?}");
    }

    #[test]
    fn banned_words_in_any_spelling_are_caught() {
        let bad = format!("{GOOD_TE} ఇది ఒక టోకెన్ సమస్య.");
        assert_eq!(check(&bad, te(), None).banned, vec!["టోకెన్"]);
        let bad_en = format!("{GOOD_TE} The Transformer is odd.");
        assert!(check(&bad_en, te(), None).banned.contains(&"transformer".to_string()));
        let hindi = "यह एक टोकन की समस्या है। यह दूसरा वाक्य है। तीसरा वाक्य है। चौथा वाक्य है।";
        assert_eq!(check(hindi, hi(), None).banned, vec!["टोकन"]);
        // the same Devanagari spelling is banned in Marathi
        assert!(check("हे एक टोकन आहे. दुसरे वाक्य. तिसरे वाक्य. चौथे वाक्य.", Lang::get("mr").unwrap(), None).banned.contains(&"टोकन".to_string()));
    }

    #[test]
    fn long_sentences_use_the_languages_own_limit() {
        let sixteen = vec!["పదం"; 16].join(" ");
        assert_eq!(check(&format!("{sixteen}. ఒకటి. రెండు. మూడు."), te(), None).long_sentences.len(), 1);
        let fourteen = vec!["शब्द"; 14].join(" ");
        assert!(check(&format!("{fourteen}। एक। दो। तीन।"), hi(), None).long_sentences.is_empty());
        // Malayalam is stricter (12): the same fourteen words are too many there
        let ml = vec!["വാക്ക്"; 14].join(" ");
        assert_eq!(check(&format!("{ml}. ഒന്ന്. രണ്ട്. മൂന്ന്."), Lang::get("ml").unwrap(), None).long_sentences.len(), 1);
    }

    #[test]
    fn script_and_numbers_are_checked() {
        let english = "This is plain English text. It has four sentences. Not Telugu at all. Really.";
        assert!(check(english, te(), None).wrong_script);
        assert!(!check(english, Lang::english(), None).wrong_script);
        let v = check(GOOD_TE, te(), Some(&facts(64, &[])));
        assert_eq!(v.invented_numbers, vec!["14"], "14 is not in the facts here");
        let missing = check("ఒకటి. రెండు. మూడు. నాలుగు.", te(), Some(&facts(64, &[])));
        assert!(missing.missing_score);
        let latin = format!("{GOOD_TE} OK fine yes no maybe indeed");
        assert!(check(&latin, te(), None).latin_heavy);
    }

    #[test]
    fn clean_strips_markup() {
        assert_eq!(clean("**బోల్డ్**\n- ఒకటి\n- రెండు 😀\n\n1. మూడు"), "బోల్డ్ ఒకటి రెండు మూడు");
    }

    #[test]
    fn long_sentences_are_cut_near_the_middle() {
        let s = format!("{} కానీ {}.", vec!["పదం"; 9].join(" "), vec!["మాట"; 9].join(" "));
        let fixed = split_long(&s, te());
        let parts = split_sentences(&fixed);
        assert_eq!(parts.len(), 2, "{fixed}");
        assert!(parts.iter().all(|p| word_count(p) <= 15));
        // nothing to cut at: returned whole
        let blob = format!("{}.", vec!["పదం"; 20].join(" "));
        assert_eq!(split_long(&blob, te()), blob);
    }

    #[test]
    fn feedback_names_every_problem() {
        let v = Violations { banned: vec!["టోకెన్".into()], long_sentences: vec!["x".into()], max_words: 15, too_short: true, missing_score: true, ..Default::default() };
        let f = v.feedback();
        assert!(f.contains("టోకెన్") && f.contains("15 words") && f.contains("too short") && f.contains("out of 100"));
        assert!(Violations::default().feedback().is_empty());
    }
}
