use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use unicode_general_category::{get_general_category, GeneralCategory as G};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Script {
    Telugu,
    Devanagari,
    Latin,
    Tamil,
    Kannada,
    Malayalam,
    Bengali,
    Gujarati,
    Gurmukhi,
    Odia,
    Arabic,
    OlChiki,
    Meitei,
}

impl Script {
    pub fn of(c: char) -> Option<Script> {
        let cp = c as u32;
        Some(match cp {
            0x0900..=0x097F => Script::Devanagari,
            0x0980..=0x09FF => Script::Bengali,
            0x0A00..=0x0A7F => Script::Gurmukhi,
            0x0A80..=0x0AFF => Script::Gujarati,
            0x0B00..=0x0B7F => Script::Odia,
            0x0B80..=0x0BFF => Script::Tamil,
            0x0C00..=0x0C7F => Script::Telugu,
            0x0C80..=0x0CFF => Script::Kannada,
            0x0D00..=0x0D7F => Script::Malayalam,
            0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Script::Arabic,
            0x1C50..=0x1C7F => Script::OlChiki,
            0xAAE0..=0xAAFF | 0xABC0..=0xABFF => Script::Meitei,
            0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F | 0x1E00..=0x1EFF if c.is_alphabetic() => Script::Latin,
            _ => return None,
        })
    }

    /// The first and last code point of the script's Unicode block (Latin returns Basic Latin).
    pub fn block(self) -> (u32, u32) {
        match self {
            Script::Devanagari => (0x0900, 0x097F),
            Script::Bengali => (0x0980, 0x09FF),
            Script::Gurmukhi => (0x0A00, 0x0A7F),
            Script::Gujarati => (0x0A80, 0x0AFF),
            Script::Odia => (0x0B00, 0x0B7F),
            Script::Tamil => (0x0B80, 0x0BFF),
            Script::Telugu => (0x0C00, 0x0C7F),
            Script::Kannada => (0x0C80, 0x0CFF),
            Script::Malayalam => (0x0D00, 0x0D7F),
            Script::Latin => (0x0020, 0x007F),
            Script::Arabic => (0x0600, 0x06FF),
            Script::OlChiki => (0x1C50, 0x1C7F),
            Script::Meitei => (0xABC0, 0xABFF),
        }
    }
}

/// One-letter Unicode major class: L letters, M marks, N numbers, P punctuation, S symbols, Z separators, C other.
pub fn major_class(c: char) -> char {
    match get_general_category(c) {
        G::UppercaseLetter | G::LowercaseLetter | G::TitlecaseLetter | G::ModifierLetter | G::OtherLetter => 'L',
        G::NonspacingMark | G::SpacingMark | G::EnclosingMark => 'M',
        G::DecimalNumber | G::LetterNumber | G::OtherNumber => 'N',
        G::ConnectorPunctuation
        | G::DashPunctuation
        | G::OpenPunctuation
        | G::ClosePunctuation
        | G::InitialPunctuation
        | G::FinalPunctuation
        | G::OtherPunctuation => 'P',
        G::MathSymbol | G::CurrencySymbol | G::ModifierSymbol | G::OtherSymbol => 'S',
        G::SpaceSeparator | G::LineSeparator | G::ParagraphSeparator => 'Z',
        _ => 'C',
    }
}

/// Zero-width joiners and other invisible format characters.
pub fn is_format(c: char) -> bool {
    get_general_category(c) == G::Format
}

/// How many letters (and the marks that belong to them) each script contributes.
pub fn script_counts(text: &str) -> HashMap<Script, usize> {
    let mut counts = HashMap::new();
    for c in text.chars() {
        if matches!(major_class(c), 'L' | 'M') {
            if let Some(s) = Script::of(c) {
                *counts.entry(s).or_insert(0) += 1;
            }
        }
    }
    counts
}

pub fn dominant_script(text: &str) -> Option<Script> {
    script_counts(text).into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(s, _)| s)
}

/// Share of the letters in `text` that belong to `script`, 0.0 when there are no letters at all.
pub fn script_share(text: &str, script: Script) -> f64 {
    let counts = script_counts(text);
    let total: usize = counts.values().sum();
    if total == 0 {
        return 0.0;
    }
    *counts.get(&script).unwrap_or(&0) as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_told_apart() {
        assert_eq!(dominant_script("ఒక కిలో బియ్యం ధర ఎంత?"), Some(Script::Telugu));
        assert_eq!(dominant_script("चावल की कीमत क्या है?"), Some(Script::Devanagari));
        assert_eq!(dominant_script("வணக்கம் நண்பரே"), Some(Script::Tamil));
        assert_eq!(dominant_script("what is the price?"), Some(Script::Latin));
        assert_eq!(dominant_script("1234 ?!"), None);
        assert!(script_share("ఒక కిలో rice", Script::Telugu) > 0.5);
    }

    #[test]
    fn marks_count_with_their_letters() {
        // a vowel sign on its own is a mark, not a letter, and must still count as Telugu
        assert_eq!(script_counts("ి").get(&Script::Telugu), Some(&1));
        assert_eq!(major_class('ి'), 'M');
        assert_eq!(major_class('।'), 'P');
        assert_eq!(major_class('₹'), 'S');
        assert_eq!(major_class('\u{200d}'), 'C');
    }
}
