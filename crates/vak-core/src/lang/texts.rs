use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// The hand-written (Telugu, Hindi) or machine-translated wording a language uses for audit write-ups.
#[derive(Clone, Debug, Deserialize)]
pub struct Bank {
    pub model: String,
    pub what: String,
    pub total: String,
    pub changed: String,
    pub unchanged: String,
    pub score: String,
    pub no_flags: String,
    pub top_word: String,
    pub next: String,
    pub concern: Vec<String>,
    pub mixed: Vec<String>,
    pub steady: Vec<String>,
    pub flags: HashMap<String, String>,
    pub steps: HashMap<String, String>,
    #[serde(default)]
    pub not_tested: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PoliteVariant {
    pub prefix: String,
    pub suffix: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VariantText {
    pub distractors: Vec<String>,
    pub controls: Vec<String>,
    pub polite: Vec<PoliteVariant>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Notices {
    pub ai_translation: String,
    pub not_advice: String,
    pub privacy: String,
    pub confirm_prompt: String,
    pub please_repeat: String,
    pub say_again_slowly: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LangText {
    pub code: String,
    #[serde(default)]
    pub machine_translated: bool,
    pub bank: Bank,
    pub variants: VariantText,
    pub notices: Notices,
    pub jargon: Vec<String>,
    #[serde(default)]
    pub style_example: Option<String>,
}

/// English words the explanation must never lean on, wherever they come from.
pub const ENGLISH_JARGON: [&str; 7] = ["transformer", "embedding", "attention", "neural network", "neural net", "logit", "token"];

#[derive(Deserialize)]
struct EnglishVariants {
    distractors: Vec<String>,
    polite_prefix: String,
    polite_suffix: String,
    controls: Vec<String>,
}

#[derive(Deserialize)]
struct EnglishFile {
    bank: Bank,
    variants: EnglishVariants,
    notices: Notices,
}

fn english() -> LangText {
    let f: EnglishFile = serde_json::from_str(include_str!("../../data/en_sources.json")).expect("en_sources.json is valid");
    LangText {
        code: "en".into(),
        machine_translated: false,
        bank: f.bank,
        variants: VariantText {
            distractors: f.variants.distractors,
            controls: f.variants.controls,
            polite: vec![
                PoliteVariant { prefix: format!("{} ", f.variants.polite_prefix), suffix: String::new() },
                PoliteVariant { prefix: String::new(), suffix: format!(" {}", f.variants.polite_suffix) },
            ],
        },
        notices: f.notices,
        jargon: vec![],
        style_example: None,
    }
}

macro_rules! texts {
    ($($code:literal => $path:literal),+ $(,)?) => {
        vec![$(($code, include_str!($path))),+]
    };
}

static TEXTS: LazyLock<HashMap<String, LangText>> = LazyLock::new(|| {
    let sources: Vec<(&str, &str)> = texts![
        "te" => "../../data/handwritten/te.json",
        "hi" => "../../data/handwritten/hi.json",
        "ta" => "../../data/generated/ta.json",
        "kn" => "../../data/generated/kn.json",
        "ml" => "../../data/generated/ml.json",
        "mr" => "../../data/generated/mr.json",
        "bn" => "../../data/generated/bn.json",
        "gu" => "../../data/generated/gu.json",
        "pa" => "../../data/generated/pa.json",
        "od" => "../../data/generated/od.json",
    ];
    let mut map: HashMap<String, LangText> = sources
        .into_iter()
        .map(|(code, raw)| {
            let t: LangText = serde_json::from_str(raw).unwrap_or_else(|e| panic!("{code}.json: {e}"));
            (code.to_string(), t)
        })
        .collect();
    map.insert("en".into(), english());
    map
});

/// Wording for a language; `None` for the languages that are speech-and-translation only.
pub fn text_for(code: &str) -> Option<&'static LangText> {
    TEXTS.get(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::audit::scoring::flag_codes;

    #[test]
    fn every_full_language_has_complete_wording() {
        for l in Lang::full() {
            let t = text_for(&l.code).unwrap_or_else(|| panic!("no text for {}", l.code));
            for code in flag_codes() {
                assert!(t.bank.flags.contains_key(*code), "{}: missing flag text {code}", l.code);
                assert!(t.bank.steps.contains_key(*code), "{}: missing step text {code}", l.code);
            }
            assert!(t.bank.steps.contains_key("default"), "{}", l.code);
            assert!(t.bank.steady.len() >= 2 && !t.bank.concern.is_empty() && !t.bank.mixed.is_empty(), "{}", l.code);
            assert_eq!(t.variants.distractors.len(), 3, "{}", l.code);
            assert!(t.variants.controls.len() >= 2 && t.variants.polite.len() == 2, "{}", l.code);
            for tpl in [&t.bank.total, &t.bank.changed] {
                assert!(tpl.contains("{n}"), "{}: lost the count slot in {tpl}", l.code);
            }
            assert!(t.bank.score.contains("{n}") && t.bank.top_word.contains("{w}"), "{}", l.code);
            if l.code != "en" {
                assert!(t.jargon.len() >= 6, "{}: jargon list too short", l.code);
            }
        }
    }

    #[test]
    fn only_te_hi_en_are_hand_written() {
        let hand: Vec<_> = Lang::full().filter(|l| !text_for(&l.code).unwrap().machine_translated).map(|l| l.code.clone()).collect();
        assert_eq!(hand, vec!["te", "hi", "en"]);
    }
}
