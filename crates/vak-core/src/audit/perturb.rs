//! The perturbation suite: ways to change a prompt that should (or should not) change the answer.
//! Spelling slips are the kind real typists make (short/long vowel mix-ups, a dropped anusvara or nukta),
//! not random character noise. Everything here is deterministic for a given prompt.

use serde_json::Value;

use super::types::{TestKind, Variant};
use crate::lang::{text_for, Lang};
use crate::rng::Rng;
use crate::text::{
    dominant_script, has_ascii_digits, has_native_digits, norm, script_share, tidy_spaces, to_ascii_digits, to_native_digits, trim_punct, word_spans,
    Script,
};

fn push_pair(out: &mut Vec<(char, char, String)>, a: u32, b: u32) {
    if let (Some(x), Some(y)) = (char::from_u32(a), char::from_u32(b)) {
        out.push((x, y, format!("{x}→{y}")));
        out.push((y, x, format!("{y}→{x}")));
    }
}

/// Swaps and drops typists really make, derived from the script's Unicode block: every Brahmic block
/// keeps the i/ii, u/uu (and for southern scripts e/ee, o/oo) vowel signs at the same offsets.
fn script_slips(script: Script, lang_code: &str) -> (Vec<(char, char, String)>, Vec<char>) {
    let base = script.block().0;
    let mut swaps = vec![];
    push_pair(&mut swaps, base + 0x3F, base + 0x40);
    push_pair(&mut swaps, base + 0x41, base + 0x42);
    if matches!(script, Script::Telugu | Script::Tamil | Script::Kannada | Script::Malayalam) {
        push_pair(&mut swaps, base + 0x46, base + 0x47);
        push_pair(&mut swaps, base + 0x4A, base + 0x4B);
    }
    let mut drops = vec![];
    if !matches!(script, Script::Tamil | Script::Latin | Script::Arabic | Script::OlChiki | Script::Meitei) {
        drops.extend(char::from_u32(base + 0x02));
    }
    if matches!(script, Script::Devanagari | Script::Bengali | Script::Gurmukhi | Script::Gujarati | Script::Odia) {
        drops.extend(char::from_u32(base + 0x3C));
    }
    match lang_code {
        "te" => {
            swaps.push(('ల', 'ళ', "ల→ళ".into()));
            swaps.push(('న', 'ణ', "న→ణ".into()));
        }
        "hi" => {
            swaps.push(('श', 'ष', "श→ष".into()));
            swaps.push(('ण', 'न', "ण→न".into()));
        }
        _ => {}
    }
    (swaps, drops)
}

fn indic_options(word: &str, script: Script, lang_code: &str) -> Vec<(String, String)> {
    let (swaps, drops) = script_slips(script, lang_code);
    let mut options = vec![];
    let chars: Vec<char> = word.chars().collect();
    for (a, b, why) in &swaps {
        for (i, c) in chars.iter().enumerate() {
            if c == a {
                let mut w = chars.clone();
                w[i] = *b;
                options.push((w.into_iter().collect(), why.clone()));
            }
        }
    }
    for d in &drops {
        for (i, c) in chars.iter().enumerate() {
            if c == d {
                let mut w = chars.clone();
                w.remove(i);
                options.push((w.into_iter().collect(), format!("dropped {d}")));
            }
        }
    }
    options
}

fn latin_options(word: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 4 || !chars.iter().all(|c| c.is_alphabetic()) {
        return vec![];
    }
    let mut options = vec![];
    for i in 1..chars.len() - 2 {
        if chars[i] != chars[i + 1] {
            let mut w = chars.clone();
            w.swap(i, i + 1);
            options.push((w.into_iter().collect(), "swapped two letters".to_string()));
        }
    }
    for i in 1..chars.len() - 1 {
        let mut w = chars.clone();
        w.remove(i);
        options.push((w.into_iter().collect(), "missing letter".to_string()));
    }
    options
}

pub fn typo_variants(prompt: &str, count: usize, rng: &mut Rng, lang: Lang) -> Vec<Variant> {
    struct Pool {
        start: usize,
        end: usize,
        word: String,
        core: String,
        options: Vec<(String, String)>,
    }
    let mut pools: Vec<Pool> = vec![];
    for sp in word_spans(prompt) {
        let word = &prompt[sp.start..sp.end];
        let core = trim_punct(word);
        if core.chars().count() < 3 {
            continue;
        }
        let options = match dominant_script(core) {
            Some(Script::Latin) => latin_options(core),
            Some(s) => indic_options(core, s, &lang.code),
            None => vec![],
        };
        if !options.is_empty() {
            pools.push(Pool { start: sp.start, end: sp.end, word: word.into(), core: core.into(), options });
        }
    }
    if pools.is_empty() {
        return vec![];
    }
    let mut out: Vec<Variant> = vec![];
    let mut seen = vec![prompt.to_string()];
    for _ in 0..60 {
        if out.len() >= count {
            break;
        }
        let slips = if out.is_empty() { 1 } else { 1 + rng.below(2) };
        let mut picks = rng.sample_indices(pools.len(), slips.min(pools.len()));
        picks.sort_by_key(|&i| std::cmp::Reverse(pools[i].start));
        let mut text = prompt.to_string();
        let mut notes = vec![];
        for &i in &picks {
            let p = &pools[i];
            let (new_core, why) = rng.pick(&p.options).clone();
            text = format!("{}{}{}", &text[..p.start], p.word.replacen(&p.core, &new_core, 1), &text[p.end..]);
            notes.push(format!("{} → {} ({})", p.core, new_core, why));
        }
        if !seen.contains(&text) {
            seen.push(text.clone());
            notes.reverse();
            let label = if picks.len() == 1 { "A small spelling slip" } else { "Two small spelling slips" };
            out.push(Variant::new(format!("typo-{}", out.len() + 1), "typo", TestKind::Invariance, label, text).with_note(notes.join("; ")));
        }
    }
    out
}

pub fn format_variants(prompt: &str, lang: Lang, count: usize) -> Vec<Variant> {
    let words: Vec<&str> = prompt.split_whitespace().collect();
    let mut cands: Vec<(&str, String)> = vec![];
    if words.len() >= 2 {
        cands.push(("Extra spaces between words", words.join("  ")));
    }
    let tail = prompt.trim_end().trim_end_matches([' ', '?', '.', '!', '।']);
    if tail != prompt.trim_end() {
        cands.push(("Question mark or full stop removed", tail.to_string()));
    } else {
        cands.push(("A full stop added at the end", format!("{}{}", prompt.trim_end(), lang.sentence_end)));
    }
    if prompt.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3 {
        cands.push(("All lowercase", prompt.to_lowercase()));
        cands.push(("ALL CAPITALS", prompt.to_uppercase()));
    }
    if words.len() >= 4 {
        let mid = words.len() / 2;
        cands.push(("A line break in the middle", format!("{}\n{}", words[..mid].join(" "), words[mid..].join(" "))));
    }
    if lang.sentence_end == "।" && prompt.contains('।') {
        cands.push(("Full stop instead of danda", prompt.replace('।', ".")));
    }
    let base = norm(prompt);
    let mut seen = vec![prompt.to_string(), tidy_spaces(prompt)];
    let mut out = vec![];
    for (label, text) in cands {
        // only keep edits that are formatting and nothing else
        if seen.contains(&text) || norm(&text) != base {
            continue;
        }
        seen.push(text.clone());
        out.push(Variant::new(format!("format-{}", out.len() + 1), "format", TestKind::Invariance, label, text));
        if out.len() >= count {
            break;
        }
    }
    out
}

pub fn numeral_variant(prompt: &str, lang: Lang) -> Option<Variant> {
    if let Some(zero) = lang.digits_zero.filter(|_| has_ascii_digits(prompt)) {
        return Some(Variant::new(
            "numerals-1",
            "numerals",
            TestKind::Invariance,
            format!("Digits written as {} numerals", lang.english),
            to_native_digits(prompt, zero),
        ));
    }
    has_native_digits(prompt).then(|| Variant::new("numerals-1", "numerals", TestKind::Invariance, "Digits written as 0-9", to_ascii_digits(prompt)))
}

fn lang_text(lang: Lang) -> &'static crate::lang::LangText {
    text_for(&lang.code).or_else(|| text_for("en")).expect("english wording exists")
}

pub fn distractor_variants(prompt: &str, lang: Lang, count: usize) -> Vec<Variant> {
    lang_text(lang)
        .variants
        .distractors
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, line)| {
            let before = i % 2 == 0;
            let text = if before { format!("{line} {prompt}") } else { format!("{prompt} {line}") };
            Variant::new(format!("distractor-{}", i + 1), "distractor", TestKind::Invariance, format!("An unrelated remark {} the question", if before { "before" } else { "after" }), text)
                .with_note(line.clone())
        })
        .collect()
}

pub fn polite_variants(prompt: &str, lang: Lang, count: usize) -> Vec<Variant> {
    lang_text(lang)
        .variants
        .polite
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, p)| {
            let label = if p.prefix.is_empty() { "A thank-you added" } else { "Politely worded" };
            Variant::new(format!("polite-{}", i + 1), "polite", TestKind::Invariance, label, format!("{}{}{}", p.prefix, prompt, p.suffix).trim().to_string())
        })
        .collect()
}

pub fn control_variants(lang: Lang, count: usize) -> Vec<Variant> {
    lang_text(lang)
        .variants
        .controls
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, t)| {
            Variant::new(format!("control-{}", i + 1), "control", TestKind::Sensitivity, "A completely different question", t.clone())
                .with_note("sanity check: the answer should change")
        })
        .collect()
}

/// One call writes both the rewordings and the meaning-flipped versions, so the instructions are paid for once.
pub fn rewrite_system(paraphrases: usize, opposites: usize) -> String {
    let mut s = String::from(
        "You rewrite a user's message. Keep the same language and script as the original. Never answer the message.\n",
    );
    if paraphrases > 0 {
        s += &format!(
            "\"paraphrases\": write {paraphrases} different rewrites that ask for exactly the same thing. Keep every fact, number and name. Change wording and sentence structure, not meaning.\n"
        );
    }
    if opposites > 0 {
        s += &format!(
            "\"opposites\": write {opposites} minimal counterfactuals, so the correct answer would be different: flip the key fact, add a negation, or reverse the sentiment or condition, changing as few words as possible.\n"
        );
    }
    s + "Reply with JSON only: {\"paraphrases\": [\"...\"], \"opposites\": [\"...\"]}"
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|s| s.as_str()).map(|s| s.trim().to_string()).collect()).unwrap_or_default()
}

/// Keep rewrites that are real changes, similar in size, and in the right script.
pub fn accept_rewrites(prompt: &str, lang: Lang, items: Vec<String>, n: usize) -> Vec<String> {
    let base = norm(prompt);
    let len = prompt.chars().count() as f64;
    let mut out: Vec<String> = vec![];
    for item in items {
        let l = item.chars().count() as f64;
        if item.is_empty() || norm(&item) == base || out.iter().any(|o| norm(o) == norm(&item)) {
            continue;
        }
        if !(0.4 * len <= l && l <= 2.6 * len + 20.0) {
            continue;
        }
        if lang.script != Script::Latin && script_share(&item, lang.script) < 0.5 {
            continue;
        }
        out.push(item);
    }
    out.truncate(n);
    out
}

/// (paraphrase variants, negation variants) from the helper model's JSON reply.
pub fn rewrite_variants(prompt: &str, lang: Lang, reply: &Value, n_para: usize, n_opp: usize) -> (Vec<Variant>, Vec<Variant>) {
    let paras = accept_rewrites(prompt, lang, strings(reply.get("paraphrases")), n_para);
    let opps = accept_rewrites(prompt, lang, strings(reply.get("opposites")), n_opp);
    (
        paras.into_iter().enumerate().map(|(i, t)| Variant::new(format!("paraphrase-{}", i + 1), "paraphrase", TestKind::Invariance, "Said in different words", t)).collect(),
        opps.into_iter()
            .enumerate()
            .map(|(i, t)| Variant::new(format!("negation-{}", i + 1), "negation", TestKind::Sensitivity, "Meaning flipped on purpose", t).with_note("sanity check: the answer should change"))
            .collect(),
    )
}

pub fn roman_variant(prompt: &str, typed: &str) -> Option<Variant> {
    (norm(typed) != norm(prompt) && !typed.trim().is_empty())
        .then(|| Variant::new("script-1", "script", TestKind::Invariance, "Typed in English letters, like on a phone", typed.trim().to_string()))
}

pub fn language_variant(dst: Lang, prompt: &str, translated: &str) -> Option<Variant> {
    (norm(translated) != norm(prompt) && !translated.trim().is_empty())
        .then(|| Variant::new(format!("language-{}", dst.code), "language", TestKind::Invariance, format!("Asked in {}", dst.english), translated.trim().to_string()))
}

/// Which languages to translate a prompt into for the "other languages" tests.
pub fn language_targets(from: Lang, count: usize) -> Vec<Lang> {
    let order: &[&str] = match from.code.as_str() {
        "te" => &["hi", "en"],
        "hi" => &["te", "en"],
        "en" => &["hi", "te"],
        _ => &["en", "hi"],
    };
    order.iter().filter_map(|c| Lang::get(c)).filter(|l| *l != from).take(count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn te() -> Lang {
        Lang::get("te").unwrap()
    }
    const Q: &str = "ఒక కిలో బియ్యం ధర 60 రూపాయలు అయితే మూడు కిలోల ధర ఎంత?";

    #[test]
    fn typos_are_deterministic_and_real_edits() {
        let a = typo_variants(Q, 3, &mut Rng::from_text("perturb", Q), te());
        let b = typo_variants(Q, 3, &mut Rng::from_text("perturb", Q), te());
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(a.iter().all(|v| v.text != Q && !v.note.is_empty()));
        let mut texts: Vec<_> = a.iter().map(|v| &v.text).collect();
        texts.dedup();
        assert_eq!(texts.len(), a.len());
    }

    #[test]
    fn typos_work_in_every_script_and_latin() {
        for (code, q) in [("hi", "दिल्ली से हैदराबाद की ट्रेन में कितने घंटे लगते हैं?"), ("ta", "இந்தப் படம் நன்றாக இருக்கிறது"), ("ml", "ഈ സിനിമ വളരെ നല്ലതാണ്"), ("bn", "এই সিনেমাটা খুব ভালো")] {
            let l = Lang::get(code).unwrap();
            assert!(!typo_variants(q, 2, &mut Rng::from_text("p", q), l).is_empty(), "{code}");
        }
        let en = typo_variants("please check the weather tomorrow", 2, &mut Rng::new(1), Lang::english());
        assert!(en.iter().any(|v| v.note.contains("letter")));
    }

    #[test]
    fn format_edits_never_change_meaning() {
        let v = format_variants(Q, te(), 3);
        assert!(!v.is_empty());
        assert!(v.iter().all(|x| norm(&x.text) == norm(Q) && x.text != Q));
        let hi = Lang::get("hi").unwrap();
        assert!(format_variants("यह क्या है।", hi, 5).iter().any(|x| x.label.contains("danda")));
    }

    #[test]
    fn numerals_go_both_ways() {
        assert_eq!(numeral_variant("ధర 60", te()).unwrap().text, "ధర ౬౦");
        assert_eq!(numeral_variant("ధర ౬౦", te()).unwrap().text, "ధర 60");
        assert!(numeral_variant("ధర", te()).is_none());
        assert!(numeral_variant("price 60", Lang::english()).is_none());
    }

    #[test]
    fn fixed_variants_use_the_languages_own_wording() {
        assert_eq!(distractor_variants(Q, te(), 2).len(), 2);
        assert!(distractor_variants(Q, te(), 1)[0].text.ends_with(Q) || distractor_variants(Q, te(), 1)[0].text.contains(Q));
        assert_eq!(polite_variants(Q, te(), 2).len(), 2);
        assert_eq!(control_variants(Lang::get("ta").unwrap(), 2).len(), 2);
        assert!(control_variants(te(), 1)[0].kind == TestKind::Sensitivity);
    }

    #[test]
    fn rewrites_are_filtered() {
        let reply = json!({"paraphrases": [Q, "ఒక కిలో బియ్యం 60 రూపాయలైతే, మూడు కిలోలకు ఎంత అవుతుంది?", "x", "English answer only here that is long enough to pass the length check ok"], "opposites": ["ఒక కిలో బియ్యం ధర 60 రూపాయలు కాకపోతే మూడు కిలోల ధర ఎంత?"]});
        let (p, o) = rewrite_variants(Q, te(), &reply, 3, 1);
        assert_eq!((p.len(), o.len()), (1, 1), "{p:?}");
        assert_eq!(o[0].kind, TestKind::Sensitivity);
        assert!(rewrite_system(2, 1).contains("paraphrases") && !rewrite_system(0, 1).contains("different rewrites"));
    }

    #[test]
    fn language_targets_skip_the_source() {
        let t: Vec<_> = language_targets(te(), 2).iter().map(|l| l.code.clone()).collect();
        assert_eq!(t, vec!["hi", "en"]);
        assert_eq!(language_targets(Lang::english(), 1)[0].code, "hi");
        assert!(language_variant(Lang::get("hi").unwrap(), Q, Q).is_none());
    }
}
