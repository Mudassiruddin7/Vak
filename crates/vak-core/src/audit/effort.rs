//! How much work an audit gets. Effort is the dial people turn; difficulty is what the prompt itself
//! suggests. Auto mode reads the difficulty and picks the effort, so a one-line greeting does not get
//! the same treatment as a three-part rupee calculation.

use serde::{Deserialize, Serialize};

use crate::text::{dominant_script, major_class, numbers_in, script_counts, Script};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Light,
    Balanced,
    Thorough,
}

impl<'de> Deserialize<'de> for Effort {
    /// Accepts the new names and the older depth names, so saved audits and old clients keep working.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Effort::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown effort {s:?}")))
    }
}

impl Effort {
    pub fn parse(s: &str) -> Option<Effort> {
        match s.trim().to_lowercase().as_str() {
            "light" | "quick" => Some(Effort::Light),
            "balanced" | "standard" => Some(Effort::Balanced),
            "thorough" | "deep" => Some(Effort::Thorough),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Light => "light",
            Effort::Balanced => "balanced",
            Effort::Thorough => "thorough",
        }
    }

    /// The name the browser's older depth switch used.
    pub fn depth_name(self) -> &'static str {
        match self {
            Effort::Light => "quick",
            Effort::Balanced => "standard",
            Effort::Thorough => "deep",
        }
    }

    pub fn preset(self) -> Preset {
        match self {
            Effort::Light => Preset {
                resamples: 2, typo: 1, format: 1, paraphrase: 1, language: 1, distractor: 1, polite: 0, negation: 1, control: 1, units: 4,
                first_wave: 1, adaptive: false, judge_reasoning: None, explain_attempts: 2,
            },
            Effort::Balanced => Preset {
                resamples: 3, typo: 3, format: 2, paraphrase: 3, language: 2, distractor: 2, polite: 1, negation: 1, control: 1, units: 8,
                first_wave: 1, adaptive: true, judge_reasoning: None, explain_attempts: 3,
            },
            Effort::Thorough => Preset {
                resamples: 5, typo: 5, format: 3, paraphrase: 5, language: 2, distractor: 3, polite: 2, negation: 2, control: 2, units: 16,
                first_wave: 99, adaptive: false, judge_reasoning: Some("low"), explain_attempts: 3,
            },
        }
    }
}

/// Upper bounds per category. `first_wave` is how many of each category run before any signal is seen;
/// with `adaptive` on, a category only gets more tests when its first ones came back less than clean.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub resamples: usize,
    pub typo: usize,
    pub format: usize,
    pub paraphrase: usize,
    pub language: usize,
    pub distractor: usize,
    pub polite: usize,
    pub negation: usize,
    pub control: usize,
    pub units: usize,
    pub first_wave: usize,
    pub adaptive: bool,
    pub judge_reasoning: Option<&'static str>,
    pub explain_attempts: usize,
}

impl Preset {
    pub fn cap(&self, category: &str) -> usize {
        match category {
            "typo" => self.typo,
            "format" => self.format,
            "numerals" | "script" => 1,
            "paraphrase" => self.paraphrase,
            "language" => self.language,
            "distractor" => self.distractor,
            "polite" => self.polite,
            "negation" => self.negation,
            "control" => self.control,
            _ => usize::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Easy,
    Medium,
    Hard,
}

#[derive(Clone, Debug, Serialize)]
pub struct Difficulty {
    pub score: f64,
    pub level: Level,
    /// Short codes the interface turns into words ("numbers", "long", ...).
    pub reasons: Vec<&'static str>,
    pub suggested: Effort,
}

const CONDITIONALS: &[&str] = &[
    "if", "unless", "except", "only", "but", "not", "never", "అయితే", "ఒకవేళ", "కాదు", "లేదు", "తప్ప", "अगर", "यदि", "नहीं", "लेकिन", "सिवाय", "जबकि",
    "என்றால்", "இல்லை", "ஆனால்", "ಆದರೆ", "ಇಲ್ಲ", "ಅಲ್ಲ", "എങ്കിൽ", "അല്ല", "ഇല്ല", "जर", "नाही", "पण", "যদি", "না", "কিন্তু", "જો", "નથી", "ਜੇ", "ਨਹੀਂ", "ਪਰ", "ଯଦି", "ନାହିଁ", "କିନ୍ତୁ",
];

/// A rough read on how easily a model could go wrong on this prompt. Not a promise, just a way to avoid
/// spending the same on a greeting and on a multi-step rupee calculation.
pub fn difficulty(prompt: &str) -> Difficulty {
    let words = prompt.split_whitespace().count();
    let numbers = numbers_in(prompt).len();
    let counts = script_counts(prompt);
    let scripts = counts.values().filter(|&&n| n >= 3).count();
    let total_letters: usize = counts.values().sum();
    let latin = *counts.get(&Script::Latin).unwrap_or(&0);
    let indic_script = dominant_script(prompt).map(|s| s != Script::Latin).unwrap_or(false);
    let questions = prompt.chars().filter(|c| matches!(c, '?' | '？')).count();
    let lowered = prompt.to_lowercase();
    let conditionals = lowered
        .split(|c: char| !matches!(major_class(c), 'L' | 'M'))
        .filter(|w| !w.is_empty() && CONDITIONALS.contains(w))
        .count();
    let arithmetic = prompt.chars().any(|c| matches!(c, '+' | '×' | '÷' | '%' | '=' | '₹' | '$')) as usize;

    let mut score = 0.0f64;
    let mut reasons: Vec<&'static str> = vec![];
    if words >= 28 {
        score += 0.25;
        reasons.push("long");
    } else if words >= 14 {
        score += 0.12;
    }
    if numbers >= 3 {
        score += 0.3;
        reasons.push("numbers");
    } else if numbers >= 1 {
        score += 0.12;
        reasons.push("numbers");
    }
    if arithmetic > 0 && numbers > 0 {
        score += 0.08;
    }
    if scripts >= 2 {
        score += 0.15;
        reasons.push("mixed_script");
    }
    if indic_script && total_letters > 0 && latin * 100 / total_letters.max(1) >= 12 {
        score += 0.05;
    }
    if conditionals >= 2 {
        score += 0.2;
        reasons.push("conditions");
    } else if conditionals == 1 {
        score += 0.08;
    }
    if questions >= 2 {
        score += 0.15;
        reasons.push("multi_part");
    }
    let score = (score.min(1.0) * 100.0).round() / 100.0;
    let (level, suggested) = if score < 0.2 {
        (Level::Easy, Effort::Light)
    } else if score < 0.55 {
        (Level::Medium, Effort::Balanced)
    } else {
        (Level::Hard, Effort::Thorough)
    };
    reasons.dedup();
    Difficulty { score, level, reasons, suggested }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_depth_names_still_parse() {
        assert_eq!(Effort::parse("quick"), Some(Effort::Light));
        assert_eq!(Effort::parse("Standard"), Some(Effort::Balanced));
        assert_eq!(Effort::parse("deep"), Some(Effort::Thorough));
        assert_eq!(serde_json::from_str::<Effort>("\"deep\"").unwrap(), Effort::Thorough);
        assert!(serde_json::from_str::<Effort>("\"max\"").is_err());
    }

    #[test]
    fn presets_grow_with_effort() {
        let (l, b, t) = (Effort::Light.preset(), Effort::Balanced.preset(), Effort::Thorough.preset());
        assert!(l.units < b.units && b.units < t.units);
        assert!(l.paraphrase <= b.paraphrase && b.paraphrase < t.paraphrase);
        assert!(b.adaptive && !l.adaptive && !t.adaptive);
        assert_eq!(t.judge_reasoning, Some("low"));
        assert_eq!(b.cap("numerals"), 1);
    }

    #[test]
    fn easy_hard_and_the_middle() {
        let easy = difficulty("ఈ సినిమా బాగుంది");
        assert_eq!(easy.level, Level::Easy);
        assert_eq!(easy.suggested, Effort::Light);
        let hard = difficulty("ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే 3 కిలోల ధర ఎంత? 500 రూపాయలు ఇస్తే ఎంత మిగులుతుంది? కానీ 10% తగ్గింపు ఉంటే?");
        assert_eq!(hard.level, Level::Hard, "{:?}", hard);
        assert_eq!(hard.suggested, Effort::Thorough);
        assert!(hard.reasons.contains(&"numbers") && hard.reasons.contains(&"multi_part"));
        let mid = difficulty("दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं? 2 दिन पहले टिकट लिया");
        assert_eq!(mid.level, Level::Medium, "{:?}", mid);
    }

    #[test]
    fn mixed_scripts_count() {
        let d = difficulty("Please book टिकट for tomorrow morning");
        assert!(d.reasons.contains(&"mixed_script"));
    }
}
