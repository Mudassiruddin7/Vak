use std::ops::Deref;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::text::{dominant_script, Script};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Speech in, speech out, translation, explanations.
    Full,
    /// Speech in and translation only: Sarvam has no voice for these yet.
    Text,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voices {
    pub model: String,
    pub female: String,
    pub male: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LangInfo {
    pub code: String,
    pub bcp47: String,
    /// The code the realtime speech endpoint expects (Odia is `or-IN` there and `od-IN` everywhere else).
    pub realtime: String,
    pub english: String,
    pub native: String,
    pub script: Script,
    pub tier: Tier,
    pub digits_zero: Option<u32>,
    pub max_words: usize,
    pub sentence_end: String,
    pub tts: Option<Voices>,
    pub you: String,
    pub register: String,
    pub digits: String,
    pub script_notes: Vec<String>,
    pub loanwords: String,
    pub city: String,
}

#[derive(Deserialize)]
struct File {
    languages: Vec<LangInfo>,
}

static REGISTRY: LazyLock<Vec<LangInfo>> = LazyLock::new(|| {
    serde_json::from_str::<File>(include_str!("../../data/languages.json")).expect("languages.json is valid").languages
});

/// A handle to one entry of the registry. Cheap to copy; derefs to [`LangInfo`].
#[derive(Clone, Copy, Debug)]
pub struct Lang(&'static LangInfo);

impl Deref for Lang {
    type Target = LangInfo;
    fn deref(&self) -> &LangInfo {
        self.0
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.0.code == other.0.code
    }
}

impl Lang {
    pub fn get(code: &str) -> Option<Lang> {
        let code = code.trim().to_lowercase();
        let short = code.split(['-', '_']).next().unwrap_or("");
        // Sarvam's realtime endpoint calls Odia "or"; accept both
        let short = if short == "or" { "od" } else { short };
        REGISTRY.iter().find(|l| l.code == short).map(Lang)
    }

    pub fn all() -> impl Iterator<Item = Lang> {
        REGISTRY.iter().map(Lang)
    }

    /// The eleven languages with voices and full explanations.
    pub fn full() -> impl Iterator<Item = Lang> {
        REGISTRY.iter().filter(|l| l.tier == Tier::Full).map(Lang)
    }

    pub fn english() -> Lang {
        Lang::get("en").expect("english is in the registry")
    }

    pub fn is_english(&self) -> bool {
        self.0.code == "en"
    }

    pub fn info(&self) -> &'static LangInfo {
        self.0
    }
}

/// Best guess of the language a prompt is written in, going by script. Devanagari is read as Hindi,
/// since the script alone cannot tell Hindi from Marathi.
pub fn detect(text: &str) -> Option<Lang> {
    let code = match dominant_script(text)? {
        Script::Telugu => "te",
        Script::Devanagari => "hi",
        Script::Latin => "en",
        Script::Tamil => "ta",
        Script::Kannada => "kn",
        Script::Malayalam => "ml",
        Script::Bengali => "bn",
        Script::Gujarati => "gu",
        Script::Gurmukhi => "pa",
        Script::Odia => "od",
        Script::Arabic => "ur",
        Script::OlChiki => "sat",
        Script::Meitei => "mni",
    };
    Lang::get(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_complete_and_consistent() {
        assert_eq!(Lang::all().count(), 23);
        assert_eq!(Lang::full().count(), 11);
        for l in Lang::full() {
            let v = l.tts.as_ref().unwrap_or_else(|| panic!("{} has no voices", l.code));
            assert!(v.model.starts_with("bulbul:"), "{}", l.code);
            assert!(!v.female.is_empty() && !v.male.is_empty());
            assert!(l.bcp47.ends_with("-IN"));
            assert!(l.max_words >= 10);
        }
        assert!(Lang::all().filter(|l| l.tier == Tier::Text).all(|l| l.tts.is_none()));
    }

    #[test]
    fn lookups_forgive_formats() {
        assert_eq!(Lang::get("te-IN").unwrap().code, "te");
        assert_eq!(Lang::get("HI").unwrap().english, "Hindi");
        assert_eq!(Lang::get("or-IN").unwrap().code, "od");
        assert_eq!(Lang::get("od").unwrap().realtime, "or-IN");
        assert!(Lang::get("xx").is_none());
    }

    #[test]
    fn detection_follows_the_script() {
        assert_eq!(detect("ఈ సినిమా బాగుంది").unwrap().code, "te");
        assert_eq!(detect("यह फ़िल्म अच्छी है").unwrap().code, "hi");
        assert_eq!(detect("great film").unwrap().code, "en");
        assert_eq!(detect("இந்தப் படம் நன்றாக இருக்கிறது").unwrap().code, "ta");
        assert!(detect("123 ?!").is_none());
    }
}
