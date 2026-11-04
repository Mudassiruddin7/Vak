//! Which words of the prompt does the answer actually lean on? Occlusion: take a word (or a short run
//! of words, for long prompts) out, ask again, and see how far the answer moves.

use super::types::Unit;
use crate::text::{tidy_spaces, trim_punct, word_spans};

pub const MIN_WORDS: usize = 3;
const FLAT_FLOOR: f64 = 0.05;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSpec {
    pub index: usize,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn make_units(prompt: &str, max_units: usize) -> Vec<UnitSpec> {
    let spans: Vec<_> = word_spans(prompt).into_iter().filter(|s| !trim_punct(&prompt[s.start..s.end]).is_empty()).collect();
    if spans.len() < MIN_WORDS || max_units == 0 {
        return vec![];
    }
    let size = spans.len().div_ceil(max_units);
    spans
        .chunks(size)
        .enumerate()
        .map(|(i, g)| {
            let (start, end) = (g[0].start, g[g.len() - 1].end);
            UnitSpec { index: i, text: prompt[start..end].to_string(), start, end }
        })
        .collect()
}

pub fn without_unit(prompt: &str, u: &UnitSpec) -> String {
    tidy_spaces(&format!("{} {}", &prompt[..u.start], &prompt[u.end..]))
}

/// Turn "how far the answer moved when this word was removed" into importance (0..1, relative to the strongest).
pub fn rank_units(units: &[UnitSpec], raw_by_index: &std::collections::HashMap<usize, f64>) -> Vec<Unit> {
    let raws: Vec<f64> = units.iter().map(|u| raw_by_index.get(&u.index).map(|r| r.clamp(0.0, 1.0)).unwrap_or(0.0)).collect();
    let peak = raws.iter().cloned().fold(0.0, f64::max);
    units
        .iter()
        .zip(raws)
        .map(|(u, raw)| Unit {
            index: u.index,
            text: u.text.clone(),
            start: u.start,
            end: u.end,
            raw: (raw * 1000.0).round() / 1000.0,
            importance: if peak > FLAT_FLOOR { (raw / peak * 1000.0).round() / 1000.0 } else { 0.0 },
            changed: raw >= 0.4,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn short_prompts_are_left_whole() {
        assert!(make_units("రెండు పదాలు", 6).is_empty());
        assert!(make_units("a b c d", 0).is_empty());
    }

    #[test]
    fn units_group_words_and_cover_the_prompt() {
        let p = "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?";
        let units = make_units(p, 4);
        assert_eq!(units.len(), 4);
        assert_eq!(&p[units[0].start..units[0].end], units[0].text);
        assert!(units.windows(2).all(|w| w[0].end <= w[1].start));
        let cut = without_unit(p, &units[1]);
        assert!(!cut.contains(&units[1].text) && !cut.contains("  "));
    }

    #[test]
    fn importance_is_relative_and_flat_stays_flat() {
        let units = make_units("a bb cc dd ee ff", 6);
        let raws: HashMap<usize, f64> = [(0, 0.1), (2, 0.8)].into();
        let ranked = rank_units(&units, &raws);
        assert_eq!(ranked[2].importance, 1.0);
        assert!(ranked[2].changed && !ranked[0].changed);
        assert!(rank_units(&units, &HashMap::new()).iter().all(|u| u.importance == 0.0));
    }
}
