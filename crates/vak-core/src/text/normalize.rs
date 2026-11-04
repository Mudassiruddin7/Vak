use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use super::json::strip_think;
use super::script::{is_format, major_class};

/// Code point of the digit zero for each decimal-digit block we expect to meet.
const DIGIT_ZEROS: [u32; 12] = [
    0x0660, 0x06F0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66, 0xFF10,
];

fn digit_value(c: char) -> Option<u8> {
    if c.is_ascii_digit() {
        return Some(c as u8 - b'0');
    }
    let cp = c as u32;
    DIGIT_ZEROS.iter().find(|&&z| (z..z + 10).contains(&cp)).map(|&z| (cp - z) as u8)
}

pub fn to_ascii_digits(text: &str) -> String {
    text.chars().map(|c| digit_value(c).map(|d| (b'0' + d) as char).unwrap_or(c)).collect()
}

/// ASCII digits to the native numerals of a language. Only Telugu and Hindi (Devanagari) numerals
/// are used in everyday typing; for other languages the text is returned unchanged.
pub fn to_native_digits(text: &str, zero: u32) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { char::from_u32(zero + (c as u32 - '0' as u32)).unwrap_or(c) } else { c }).collect()
}

pub fn has_ascii_digits(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_digit())
}

pub fn has_native_digits(text: &str) -> bool {
    text.chars().any(|c| !c.is_ascii() && digit_value(c).is_some())
}

/// Comparison form: no markup, case, digit script, punctuation, joiners or extra spaces.
pub fn norm(text: &str) -> String {
    let cleaned = strip_think(text);
    let folded: String = cleaned.nfkc().collect::<String>().to_lowercase();
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for c in to_ascii_digits(&folded).chars() {
        if is_format(c) {
            continue; // zero-width joiners and friends just vanish
        }
        if c.is_whitespace() || matches!(major_class(c), 'P' | 'S' | 'Z' | 'C') {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// Long vowel sign to its short sign, per script. Typists mix these up constantly, and it rarely changes meaning.
fn short_vowel(c: char) -> Option<char> {
    let cp = c as u32;
    let short = match cp {
        0x0C40 | 0x0C42 | 0x0C47 | 0x0C4B => cp - 1, // Telugu  ీ ూ ే ో
        0x0CC0 | 0x0CC2 | 0x0CC7 | 0x0CCB => cp - 1, // Kannada
        0x0D40 | 0x0D42 | 0x0D47 | 0x0D4B => cp - 1, // Malayalam
        0x0BC0 | 0x0BC2 | 0x0BC7 | 0x0BCB => cp - 1, // Tamil
        0x0940 | 0x0942 => cp - 1,                    // Devanagari  ी ू
        0x09C0 | 0x09C2 => cp - 1,                    // Bengali
        0x0AC0 | 0x0AC2 => cp - 1,                    // Gujarati
        0x0A40 | 0x0A42 => cp - 1,                    // Gurmukhi
        0x0B40 | 0x0B42 => cp - 1,                    // Odia
        _ => return None,
    };
    char::from_u32(short)
}

/// Looser than [`norm`]: forgets vowel length, nukta and nasal marks, so "same word, spelt differently"
/// compares equal across every Indic script we support.
pub fn squash(text: &str) -> String {
    norm(text)
        .chars()
        .filter_map(|c| match c {
            '\u{0901}' => Some('\u{0902}'),       // chandrabindu -> anusvara
            '\u{0C03}' | '\u{093C}' => None,      // Telugu visarga, nukta
            other => Some(short_vowel(other).unwrap_or(other)),
        })
        .collect()
}

/// Digit runs in `text`, thousands commas removed, in ASCII digits.
pub fn numbers_in(text: &str) -> BTreeSet<String> {
    static NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?").expect("static regex"));
    let ascii: Vec<char> = to_ascii_digits(text).chars().collect();
    // western (1,000) and Indian (1,25,000) grouping both collapse; "1,2,3" stays a list
    let mut flat = String::with_capacity(ascii.len());
    for (i, &c) in ascii.iter().enumerate() {
        if c == ',' && i > 0 && ascii[i - 1].is_ascii_digit() {
            let run = ascii[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
            if (2..=3).contains(&run) {
                continue;
            }
        }
        flat.push(c);
    }
    NUM.find_iter(&flat).map(|m| m.as_str().to_string()).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Whitespace-separated words as byte ranges into `text`.
pub fn word_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                spans.push(Span { start: s, end: i });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push(Span { start: s, end: text.len() });
    }
    spans
}

/// Collapse runs of spaces and tabs, trim lines, keep single newlines.
pub fn tidy_spaces(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split([' ', '\t']).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" "))
        .collect();
    lines.join("\n").trim().to_string()
}

/// The word without punctuation or symbols glued to its edges.
pub fn trim_punct(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(major_class(c), 'P' | 'S'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_keeps_indic_words_whole() {
        // \w-based cleanup would turn these into confetti because vowel signs are combining marks
        assert_eq!(norm("బియ్యం, ధర!"), "బియ్యం ధర");
        assert_eq!(norm("क़ीमत  क्या?"), norm("क़ीमत क्या"));
        assert_eq!(norm("<think>x</think>Hello,   WORLD"), "hello world");
        assert_eq!(norm("வணக்கம், நண்பரே!"), "வணக்கம் நண்பரே");
    }

    #[test]
    fn digits_and_joiners_do_not_matter() {
        assert_eq!(norm("౧౨౩ రూపాయలు"), norm("123 రూపాయలు"));
        assert_eq!(norm("क\u{200d}ा"), norm("का"));
    }

    #[test]
    fn squash_forgives_vowel_length_in_every_script() {
        assert_eq!(squash("బీయ్యం"), squash("బియ్యం"));
        assert_eq!(squash("కిలో"), squash("కిలొ"));
        assert_eq!(squash("ठीक"), squash("ठिक"));
        assert_eq!(squash("தமிழ்நாடு பீடு"), squash("தமிழ்நாடு பிடு"));
        assert_eq!(squash("ಮೂರು"), squash("ಮುರು"));
        assert_eq!(squash("മൂന്ന്"), squash("മുന്ന്"));
        assert_ne!(squash("బియ్యం"), squash("ధర"));
    }

    #[test]
    fn digits_round_trip() {
        assert_eq!(to_native_digits("60 రూపాయలు", 0x0C66), "౬౦ రూపాయలు");
        assert_eq!(to_ascii_digits("౬౦ ६० ६০"), "60 60 60");
        assert!(has_native_digits("౬౦") && !has_native_digits("60"));
    }

    #[test]
    fn numbers_survive_grouping() {
        let n = numbers_in("1,25,000 and ౬౦ and 3.5");
        assert_eq!(n.into_iter().collect::<Vec<_>>(), vec!["125000", "3.5", "60"]);
        let m: Vec<String> = numbers_in("1,000 and 2,3,4").into_iter().collect();
        assert_eq!(m, vec!["1000", "2", "3", "4"]);
    }

    #[test]
    fn spans_and_trim() {
        let t = "ఈ  సినిమా బాగుంది,";
        let spans = word_spans(t);
        assert_eq!(spans.len(), 3);
        assert_eq!(&t[spans[2].start..spans[2].end], "బాగుంది,");
        assert_eq!(trim_punct("“బాగుంది,”"), "బాగుంది");
        assert_eq!(tidy_spaces("  a   b \n\n  c  "), "a b\n\nc");
    }
}
