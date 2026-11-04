//! Fills the system prompt for the explanation step.

use sha2::{Digest, Sha256};

use super::texts::text_for;
use super::Lang;
use crate::audit::{FaithStatus, Report};
use crate::text::{clip, trim_punct};

const TEMPLATE: &str = include_str!("../../data/explain_system.txt");

/// Everyday Indian scenes the explainer may borrow from. One is picked per audit so reports don't all sound alike.
const ANALOGIES: [&str; 6] = [
    "A chai stall that makes the same cup of chai however you order it.",
    "An auto driver who reaches the same address even when you describe it in different words.",
    "A railway platform announcement that means the same thing even with noise in the background.",
    "A shopkeeper who quotes the same price whether you ask in Telugu, Hindi or English.",
    "A tailor who stitches the same shirt from the same measurements every time.",
    "A cook whose sambar tastes the same on a rainy day and on a sunny day.",
];

fn script_name(lang: Lang) -> String {
    match lang.code.as_str() {
        "hi" | "mr" => "Devanagari script".into(),
        "en" => "English".into(),
        _ => format!("{} script", lang.english),
    }
}

fn faithfulness_line(report: &Report) -> &'static str {
    let f = &report.faithfulness;
    match (f.status, f.score) {
        (FaithStatus::Ok, Some(s)) if s >= 0.7 => "Yes. The words it named are the ones that really move its answer (rough hint).",
        (FaithStatus::Ok, Some(s)) if s >= 0.4 => "Partly. Some of the words it named matter, others do not (rough hint).",
        (FaithStatus::Ok, Some(_)) => "Not really. The words it named made little difference, while other words mattered more (rough hint, based on one try).",
        (FaithStatus::Inconclusive, _) => "Could not be checked.",
        _ => "Not checked for this model.",
    }
}

fn top_word(report: &Report) -> String {
    let Some(best) = report.units.iter().max_by(|a, b| a.raw.partial_cmp(&b.raw).unwrap_or(std::cmp::Ordering::Equal)) else {
        return "not measured".into();
    };
    if best.raw < 0.4 {
        return "none in particular; removing any single word did not change the answer".into();
    }
    format!("“{}”", trim_punct(&best.text))
}

fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// The findings and next steps of this audit, already phrased in the target language.
fn in_language(report: &Report, lang: Lang) -> (String, String) {
    let bank = &text_for(&lang.code).or_else(|| text_for("en")).expect("english wording exists").bank;
    let codes: Vec<&str> = report.flags.iter().take(3).map(|f| f.code.as_str()).collect();
    let findings: Vec<&str> = codes.iter().filter_map(|c| bank.flags.get(*c).map(|s| s.as_str())).collect();
    let steps: Vec<&str> = codes.iter().filter_map(|c| bank.steps.get(*c).map(|s| s.as_str())).collect();
    let findings = if findings.is_empty() { bank.no_flags.clone() } else { findings.join(" ") };
    let steps = if steps.is_empty() { bank.steps["default"].clone() } else { steps.join(" ") };
    (findings, steps)
}

/// A style example: hand-written for Telugu and Hindi, assembled from the bank for the others.
fn style_example(lang: Lang) -> String {
    let t = text_for(&lang.code).or_else(|| text_for("en")).expect("english wording exists");
    if let Some(ex) = &t.style_example {
        return ex.clone();
    }
    let b = &t.bank;
    [
        b.model.clone(),
        b.what.clone(),
        b.total.replace("{n}", "14"),
        b.changed.replace("{n}", "5"),
        b.score.replace("{n}", "64"),
        b.concern.join(" "),
        b.next.clone(),
        b.steps["default"].clone(),
    ]
    .join(" ")
}

pub fn build_system(report: &Report, lang: Lang, seed: &str) -> String {
    let (findings_local, steps_local) = in_language(report, lang);
    let bank = &text_for(&lang.code).or_else(|| text_for("en")).expect("english wording exists").bank;
    let flags = if report.flags.is_empty() {
        "none found in the tests that were run".to_string()
    } else {
        report.flags.iter().take(4).map(|f| format!("{} ({}): {}", f.title, f.severity.as_str(), f.detail)).collect::<Vec<_>>().join("; ")
    };
    let digest = Sha256::digest(seed.as_bytes());
    let pick = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize % ANALOGIES.len();
    let values: Vec<(&str, String)> = vec![
        ("language", lang.english.clone()),
        ("city", lang.city.clone()),
        ("max_words", lang.max_words.to_string()),
        ("script_name", script_name(lang)),
        ("you", lang.you.clone()),
        ("register", lang.register.clone()),
        ("style_example", style_example(lang)),
        ("consistency_score", format!("{:.2}", report.consistency_score)),
        ("flagged_behaviors", flags),
        ("total_perturbations", report.total_perturbations.to_string()),
        ("unexpected_changes", report.unexpected_changes.to_string()),
        (
            "what_was_tested",
            format!(
                "the question “{}” put to {}, with {} variations at {} effort",
                clip(&report.prompt, 140),
                report.target_label,
                report.total_perturbations,
                report.effort
            ),
        ),
        ("score_100", ((report.consistency_score * 100.0).round() as i64).to_string()),
        ("strengths", if report.strengths.is_empty() { "nothing stood out yet".into() } else { report.strengths.join("; ") }),
        ("top_word", top_word(report)),
        ("faithfulness", faithfulness_line(report).to_string()),
        ("not_tested", report.not_tested.iter().take(4).cloned().collect::<Vec<_>>().join("; ")),
        ("next_steps", report.next_steps.iter().take(4).cloned().collect::<Vec<_>>().join(" | ")),
        ("analogy", ANALOGIES[pick].to_string()),
        ("findings_local", findings_local),
        ("not_tested_local", bank.steady.iter().skip(1).cloned().collect::<Vec<_>>().join(" ")),
        ("steps_local", steps_local),
    ];
    let filled = fill(TEMPLATE, &values);
    // languages without writing notes of their own leave these two lines empty; drop them rather than show a bare label
    filled.lines().filter(|l| !(l.starts_with("- How to address the reader:") || l.starts_with("- Register:")) || l.trim_end().len() > l.find(':').map_or(0, |i| i + 1)).collect::<Vec<_>>().join("\n")
}

pub fn build_user(lang: Lang) -> String {
    format!("Explain these audit results to me now, in {}, following every rule above.", lang.english)
}
