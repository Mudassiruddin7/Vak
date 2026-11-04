use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::normalize::{numbers_in, squash};

/// Near-identical answers (same numbers) never need a judge.
pub const NEAR_IDENTICAL: f64 = 0.92;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Same,
    Partly,
    Different,
}

impl Verdict {
    pub fn score(self) -> f64 {
        match self {
            Verdict::Same => 1.0,
            Verdict::Partly => 0.5,
            Verdict::Different => 0.0,
        }
    }
}

fn trigrams(text: &str) -> HashMap<[char; 3], u32> {
    let padded: Vec<char> = std::iter::once(' ').chain(text.chars()).chain(std::iter::once(' ')).collect();
    let mut counts = HashMap::new();
    if padded.len() < 3 {
        counts.insert([padded.first().copied().unwrap_or(' '), padded.get(1).copied().unwrap_or(' '), ' '], 1);
        return counts;
    }
    for w in padded.windows(3) {
        *counts.entry([w[0], w[1], w[2]]).or_insert(0) += 1;
    }
    counts
}

fn cosine(a: &HashMap<[char; 3], u32>, b: &HashMap<[char; 3], u32>) -> f64 {
    let dot: f64 = a.iter().map(|(k, &v)| v as f64 * *b.get(k).unwrap_or(&0) as f64).sum();
    let na = a.values().map(|&v| (v * v) as f64).sum::<f64>().sqrt();
    let nb = b.values().map(|&v| (v * v) as f64).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

/// 0..1 from character trigrams and word overlap, after forgiving case, punctuation and vowel length.
pub fn lexical_similarity(a: &str, b: &str) -> f64 {
    let (na, nb) = (squash(a), squash(b));
    if na.is_empty() && nb.is_empty() {
        return 1.0;
    }
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let cos = cosine(&trigrams(&na), &trigrams(&nb));
    let mut wa: HashMap<&str, u32> = HashMap::new();
    let mut wb: HashMap<&str, u32> = HashMap::new();
    na.split(' ').for_each(|w| *wa.entry(w).or_insert(0) += 1);
    nb.split(' ').for_each(|w| *wb.entry(w).or_insert(0) += 1);
    let overlap: u32 = wa.iter().map(|(k, &v)| v.min(*wb.get(k).unwrap_or(&0))).sum();
    let total = wa.values().sum::<u32>() + wb.values().sum::<u32>();
    let f1 = if total == 0 { 0.0 } else { 2.0 * overlap as f64 / total as f64 };
    0.5 * cos + 0.5 * f1
}

/// True when the answers are effectively the same text, numbers included.
pub fn settled_without_judge(a: &str, b: &str) -> bool {
    if squash(a) == squash(b) {
        return true;
    }
    lexical_similarity(a, b) >= NEAR_IDENTICAL && numbers_in(a) == numbers_in(b)
}

/// A verdict from wording alone, with the lexical score behind it.
pub fn lexical_verdict(a: &str, b: &str) -> (Verdict, f64) {
    if settled_without_judge(a, b) {
        return (Verdict::Same, 1.0);
    }
    let lex = lexical_similarity(a, b);
    if lex >= 0.7 && numbers_in(a) == numbers_in(b) {
        return (Verdict::Same, lex);
    }
    if lex >= 0.4 {
        return (Verdict::Partly, lex);
    }
    (Verdict::Different, lex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_block_the_shortcut() {
        assert!(settled_without_judge("మొత్తం 180 రూపాయలు అవుతుంది", "మొత్తం 180 రూపాయలు అవుతుంది."));
        assert!(!settled_without_judge("మొత్తం 180 రూపాయలు అవుతుంది", "మొత్తం 190 రూపాయలు అవుతుంది"));
        assert_eq!(lexical_verdict("yes", "no").0, Verdict::Different);
        assert_eq!(lexical_similarity("", ""), 1.0);
    }

    #[test]
    fn vowel_length_slips_are_not_a_different_answer() {
        assert_eq!(lexical_verdict("బియ్యం ధర 60", "బీయ్యం ధర 60").0, Verdict::Same);
    }

    #[test]
    fn verdict_scores() {
        assert_eq!([Verdict::Same, Verdict::Partly, Verdict::Different].map(Verdict::score), [1.0, 0.5, 0.0]);
    }
}
