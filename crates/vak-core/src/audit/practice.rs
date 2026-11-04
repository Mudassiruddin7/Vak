//! Two tiny stand-in models so there is something to audit without spending anything. "Mood Meter" says
//! whether a short review is positive, negative or mixed. The steady one cleans its input and notices
//! negation; the shaky one reads only the start of the text and ignores negation.

use crate::text::{dominant_script, norm, squash, Script};

const SHAKY_WINDOW: usize = 42;

struct Lexicon {
    pos: &'static [&'static str],
    neg: &'static [&'static str],
    negators: &'static [&'static str],
    labels: [&'static str; 3],
}

const TE: Lexicon = Lexicon {
    pos: &["బాగుంది", "బావుంది", "బాగున్నాయి", "బాగా", "అద్భుతం", "సూపర్", "నచ్చింది", "ఇష్టం", "మంచి", "గొప్ప", "అదిరింది"],
    neg: &["చెత్త", "బాగాలేదు", "బాగోలేదు", "నిరాశ", "చెడ్డ", "బోరు", "విసుగు", "దారుణం", "నచ్చలేదు"],
    negators: &["కాదు", "లేదు", "లేవు"],
    labels: ["సానుకూలం", "ప్రతికూలం", "మిశ్రమం"],
};
const HI: Lexicon = Lexicon {
    pos: &["अच्छा", "अच्छी", "बढ़िया", "शानदार", "पसंद", "बेहतरीन", "खूब", "मज़ेदार", "कमाल"],
    neg: &["बुरा", "बुरी", "खराब", "बेकार", "निराश", "बोरिंग", "घटिया", "नापसंद", "बकवास"],
    negators: &["नहीं", "ना"],
    labels: ["सकारात्मक", "नकारात्मक", "मिला-जुला"],
};
const EN: Lexicon = Lexicon {
    pos: &["good", "great", "love", "loved", "excellent", "amazing", "nice", "wonderful", "fantastic", "awesome", "superb"],
    neg: &["bad", "awful", "hate", "hated", "boring", "terrible", "worst", "poor", "disappointing", "dull", "waste"],
    negators: &["not", "no", "never", "isnt", "wasnt", "dont", "didnt"],
    labels: ["positive", "negative", "mixed"],
};

fn lexicon(text: &str) -> &'static Lexicon {
    match dominant_script(text) {
        Some(Script::Telugu) => &TE,
        Some(Script::Devanagari) => &HI,
        _ => &EN,
    }
}

fn stem_match(word: &str, list: &[&str]) -> bool {
    let w = squash(word);
    list.iter().map(|s| squash(s)).any(|s| s.chars().count() >= 3 && (w == s || w.starts_with(&s)))
}

pub fn mood(text: &str, steady: bool) -> &'static str {
    let lx = lexicon(text);
    let pick = |i: usize| lx.labels[i];
    if steady {
        let words: Vec<String> = norm(text).replace('\'', "").split(' ').map(String::from).collect();
        let mut score = 0i32;
        for (i, w) in words.iter().enumerate() {
            let mut sign = if stem_match(w, lx.pos) { 1 } else if stem_match(w, lx.neg) { -1 } else { 0 };
            let before = words[i.saturating_sub(2)..i].iter().any(|x| lx.negators == EN.negators && stem_match(x, lx.negators));
            if sign != 0 && (before || words.iter().skip(i + 1).take(2).any(|x| stem_match(x, lx.negators))) {
                sign = -sign;
            }
            score += sign;
        }
        return pick(if score > 0 { 0 } else if score < 0 { 1 } else { 2 });
    }
    let head: String = text.chars().take(SHAKY_WINDOW).collect::<String>().to_lowercase();
    for w in head.split_whitespace() {
        let w = w.trim_matches(['.', '?', '!']);
        if lx.pos.contains(&w) {
            return pick(0);
        }
        if lx.neg.contains(&w) {
            return pick(1);
        }
    }
    pick(2)
}

pub const MODELS: [(&str, &str); 2] = [("mood-steady", "Mood Meter (steady)"), ("mood-shaky", "Mood Meter (shaky)")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_reads_negation_and_shaky_does_not() {
        assert_eq!(mood("ఈ సినిమా బాగుంది", true), "సానుకూలం");
        assert_eq!(mood("ఈ సినిమా బాగుంది కాదు", true), "ప్రతికూలం");
        assert_eq!(mood("ఈ సినిమా బాగుంది కాదు", false), "సానుకూలం");
        assert_eq!(mood("the film was not good", true), "negative");
        assert_eq!(mood("यह फ़िल्म बेकार है", true), "नकारात्मक");
        assert_eq!(mood("hello there", true), "mixed");
    }

    #[test]
    fn shaky_only_reads_the_start() {
        let pad = "word ".repeat(12);
        assert_eq!(mood(&format!("{pad}great"), false), "mixed");
        assert_eq!(mood(&format!("{pad}great"), true), "positive");
    }
}
