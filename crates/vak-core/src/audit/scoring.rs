//! From raw test outcomes to a score, flags and advice.
//!
//! The consistency score is a weighted average of how well the model behaved in each test (1 = exactly
//! as it should). Meaning-preserving edits should leave the answer alone, so a score is the similarity
//! of the two answers. Meaning-changing edits should move it, so there the score is 1 minus the similarity.

use super::types::*;
use crate::lang::Lang;
use crate::text::script_share;

pub const OK_AT: f64 = 0.8;
pub const WOBBLE_AT: f64 = 0.4;
pub const STEADY_ABOVE: f64 = 0.85;
pub const CONCERN_BELOW: f64 = 0.6;

pub fn category_weight(category: &str) -> f64 {
    match category {
        "language" | "polite" => 0.7,
        "control" => 0.5,
        _ => 1.0,
    }
}

/// (key, label, categories) in the order the groups are shown.
pub const GROUPS: [(&str, &str, &[&str]); 6] = [
    ("repeat", "Asking again", &["resample"]),
    ("noise", "Typos and formatting", &["typo", "format", "numerals", "script"]),
    ("wording", "Different wording", &["paraphrase"]),
    ("language", "Other languages", &["language"]),
    ("distraction", "Distractions and manners", &["distractor", "polite"]),
    ("meaning", "Meaning changes", &["negation", "control"]),
];

/// Fill in score and status from the similarity.
pub fn finish_test(t: &mut TestResult) {
    let (Some(_), Some(sim)) = (&t.output, t.sim) else {
        t.score = None;
        t.status = Status::Error;
        return;
    };
    let score = if t.kind == TestKind::Sensitivity { 1.0 - sim } else { sim };
    let score = (score * 1000.0).round() / 1000.0;
    t.score = Some(score);
    t.status = if score >= OK_AT {
        Status::Ok
    } else if score >= WOBBLE_AT {
        Status::Wobble
    } else {
        Status::Broke
    };
}

pub fn grade_for(score: f64) -> Grade {
    if score < CONCERN_BELOW {
        Grade::Concerning
    } else if score > STEADY_ABOVE {
        Grade::Steady
    } else {
        Grade::Mixed
    }
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// (score, tests counted, unexpected changes, tests that errored out).
pub fn consistency(tests: &[TestResult]) -> (f64, usize, usize, usize) {
    let valid: Vec<&TestResult> = tests.iter().filter(|t| t.status != Status::Error && t.score.is_some()).collect();
    let skipped = tests.len() - valid.len();
    if valid.is_empty() {
        return (0.0, 0, 0, skipped);
    }
    let wsum: f64 = valid.iter().map(|t| category_weight(&t.category)).sum();
    let total: f64 = valid.iter().map(|t| category_weight(&t.category) * t.score.unwrap_or(0.0)).sum();
    (r2(total / wsum), valid.len(), valid.iter().filter(|t| t.status == Status::Broke).count(), skipped)
}

pub fn group_scores(tests: &[TestResult]) -> Vec<Group> {
    GROUPS
        .iter()
        .filter_map(|(key, label, cats)| {
            let members: Vec<&TestResult> = tests.iter().filter(|t| cats.contains(&t.category.as_str()) && t.score.is_some()).collect();
            if members.is_empty() {
                return None;
            }
            let wsum: f64 = members.iter().map(|t| category_weight(&t.category)).sum();
            let total: f64 = members.iter().map(|t| category_weight(&t.category) * t.score.unwrap_or(0.0)).sum();
            Some(Group { key: (*key).into(), label: (*label).into(), score: r2(total / wsum), n: members.len() })
        })
        .collect()
}

fn members<'a>(tests: &'a [TestResult], cats: &[&str]) -> Vec<&'a TestResult> {
    tests.iter().filter(|t| cats.contains(&t.category.as_str()) && t.status != Status::Error).collect()
}

fn severity_for(bad: usize, n: usize) -> Severity {
    let rate = bad as f64 / n as f64;
    if rate >= 0.67 {
        Severity::High
    } else if rate >= 0.34 {
        Severity::Medium
    } else {
        Severity::Low
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

pub fn build_flags(
    tests: &[TestResult],
    units: &[Unit],
    faith: &Faithfulness,
    baseline_output: &str,
    prompt_language: Lang,
    reference_agrees: Option<bool>,
) -> Vec<Flag> {
    let mut flags: Vec<Flag> = vec![];
    let mut invariance = |code: &str, title: &str, cats: &[&str], what: &str| {
        let pool = members(tests, cats);
        let broke: Vec<&&TestResult> = pool.iter().filter(|t| t.status == Status::Broke).collect();
        if !broke.is_empty() {
            flags.push(Flag {
                code: code.into(),
                severity: severity_for(broke.len(), pool.len()),
                title: title.into(),
                detail: format!("The answer changed in {} of {} test{} where only {} was different.", broke.len(), pool.len(), plural(pool.len()), what),
                evidence: broke.iter().map(|t| t.id.clone()).collect(),
            });
        }
    };
    invariance("surface_noise", "Small typing differences change the answer", &["typo", "format", "numerals"], "spelling, spacing or digits");
    invariance(
        "script_sensitive",
        "Answer changes when typed in English letters",
        &["script"],
        "the letters used to type it (English letters instead of the native script)",
    );
    invariance("paraphrase_flip", "Rewording the question changes the answer", &["paraphrase"], "the wording");
    invariance("language_flip", "Different languages give different answers", &["language"], "the language");
    invariance("distraction_sensitive", "Side remarks throw it off", &["distractor", "polite"], "an extra unrelated or polite sentence was added");

    let repeats = members(tests, &["resample"]);
    if !repeats.is_empty() {
        let mean = repeats.iter().filter_map(|t| t.score).sum::<f64>() / repeats.len() as f64;
        if mean < 0.75 {
            flags.push(Flag {
                code: "unstable_sampling".into(),
                severity: if mean < 0.5 { Severity::High } else { Severity::Medium },
                title: "Asking the same thing again gives a different answer".into(),
                detail: format!(
                    "Asked the identical question {} more time{}, the answers agreed with the first only about {}% of the time.",
                    repeats.len(),
                    plural(repeats.len()),
                    (mean * 100.0).round() as i64
                ),
                evidence: repeats.iter().filter(|t| t.status != Status::Ok).map(|t| t.id.clone()).collect(),
            });
        }
    }

    let negations = members(tests, &["negation"]);
    let flipped: Vec<&&TestResult> = negations.iter().filter(|t| t.status == Status::Broke).collect();
    if !flipped.is_empty() {
        flags.push(Flag {
            code: "negation_blind".into(),
            severity: if flipped.len() == negations.len() { Severity::High } else { Severity::Medium },
            title: "Doesn't notice when the meaning is reversed".into(),
            detail: format!(
                "When the question was changed to mean the opposite, the answer stayed the same in {} of {} test{}.",
                flipped.len(),
                negations.len(),
                plural(negations.len())
            ),
            evidence: flipped.iter().map(|t| t.id.clone()).collect(),
        });
    }

    let same_for_unrelated: Vec<String> = members(tests, &["control"]).iter().filter(|t| t.status == Status::Broke).map(|t| t.id.clone()).collect();
    if !same_for_unrelated.is_empty() {
        flags.push(Flag {
            code: "ignores_input".into(),
            severity: Severity::High,
            title: "Gives the same answer to an unrelated question".into(),
            detail: "Given a completely different question, it still produced nearly the same answer, so the answer may not depend on what you ask.".into(),
            evidence: same_for_unrelated,
        });
    }

    let mut raws: Vec<f64> = units.iter().map(|u| u.raw).collect();
    raws.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = raws.iter().sum();
    if raws.len() >= 4 && raws[0] >= 0.5 && sum > 0.0 && raws[0] / sum >= 0.6 {
        if let Some(top) = units.iter().max_by(|a, b| a.raw.partial_cmp(&b.raw).unwrap_or(std::cmp::Ordering::Equal)) {
            flags.push(Flag {
                code: "single_word_dependence".into(),
                severity: Severity::Medium,
                title: "The answer hangs on a single word".into(),
                detail: format!("Removing just “{}” changed the answer, while taking out any other word hardly mattered.", top.text),
                evidence: vec![],
            });
        }
    }

    if faith.status == FaithStatus::Ok && faith.score.is_some_and(|s| s < 0.4) {
        flags.push(Flag {
            code: "explanation_mismatch".into(),
            severity: if faith.score.is_some_and(|s| s < 0.2) { Severity::Medium } else { Severity::Low },
            title: "Its own reasons don't match what really moves it".into(),
            detail: "The words the model said it relied on made little difference when removed, while other words mattered more. This check is a rough hint, based on a single try.".into(),
            evidence: vec![],
        });
    }

    if prompt_language.script != crate::text::Script::Latin {
        let mut outputs: Vec<&str> = vec![baseline_output];
        outputs.extend(tests.iter().filter(|t| t.kind != TestKind::Sensitivity && t.category != "language").filter_map(|t| t.output.as_deref()).filter(|o| !o.is_empty()));
        let wrong = outputs.iter().filter(|o| !o.is_empty() && script_share(o, prompt_language.script) < 0.35).count();
        if wrong as f64 / outputs.len() as f64 >= 0.5 {
            flags.push(Flag {
                code: "language_drift".into(),
                severity: Severity::Low,
                title: "Replies in a different language than the question".into(),
                detail: "Most answers came back in another language or script than the one the question was written in.".into(),
                evidence: vec![],
            });
        }
    }

    if reference_agrees == Some(false) {
        flags.push(Flag {
            code: "not_reproducible".into(),
            severity: Severity::Medium,
            title: "Today's answer doesn't match the one you saved".into(),
            detail: "The answer you gave as the reference is not what the model says now, even with the same question.".into(),
            evidence: vec![],
        });
    }
    flags.sort_by_key(|f| f.severity);
    flags
}

const ADVICE: [(&str, &str); 12] = [
    ("surface_noise", "Clean the text before it reaches the model: trim extra spaces, and normalise spelling, digits and punctuation."),
    ("script_sensitive", "Decide how phone-typed English-letter input should be handled: convert it to the native script first, or add such cases to your tests."),
    ("paraphrase_flip", "Keep several rewordings of every important question in your test set, and tighten the instructions so the answer doesn't depend on exact wording."),
    ("language_flip", "Test each key question in every language your users speak, not just one."),
    ("distraction_sensitive", "Say clearly in the instructions which part of the message is the task, and tell the model to ignore side remarks."),
    ("unstable_sampling", "Lower the randomness setting, or ask three times and keep the answer most runs agree on."),
    ("negation_blind", "Add opposite-meaning versions of your questions to your tests, since this model sometimes answers as if nothing changed."),
    ("ignores_input", "Check that your question really reaches the model, because its answer barely reacts to what you write."),
    ("single_word_dependence", "Make the instructions lean less on one keyword, and test what happens when that word is missing or misspelt."),
    ("explanation_mismatch", "Don't treat the model's own reasons as proof. Judge it by tests like these."),
    ("language_drift", "State the language of the reply in the instructions."),
    ("not_reproducible", "Re-check the answer you saved, because the model doesn't give it today."),
];
const DEFAULT_ADVICE: &str = "Keep these tests and run them again whenever you change the instructions or the model version.";

/// Every flag code the engine can raise; the wording banks must cover all of them.
pub fn flag_codes() -> &'static [&'static str] {
    &[
        "surface_noise", "script_sensitive", "paraphrase_flip", "language_flip", "distraction_sensitive", "unstable_sampling",
        "negation_blind", "ignores_input", "single_word_dependence", "explanation_mismatch", "language_drift", "not_reproducible",
    ]
}

pub fn next_step_codes(flags: &[Flag]) -> Vec<String> {
    let codes: Vec<String> = flags.iter().filter(|f| ADVICE.iter().any(|(c, _)| *c == f.code)).take(4).map(|f| f.code.clone()).collect();
    if codes.is_empty() { vec!["default".into()] } else { codes }
}

pub fn next_steps(flags: &[Flag]) -> Vec<String> {
    next_step_codes(flags)
        .iter()
        .map(|c| ADVICE.iter().find(|(k, _)| k == c).map(|(_, v)| *v).unwrap_or(DEFAULT_ADVICE).to_string())
        .collect()
}

pub fn strengths(groups: &[Group]) -> Vec<String> {
    groups.iter().filter(|g| g.score >= 0.9 && g.n >= 1).take(4).map(|g| format!("{}: held steady in {} test{}", g.label, g.n, plural(g.n))).collect()
}

const NOT_TESTED: [(&str, &str); 7] = [
    ("language", "Other languages: the question was not tried in a second language."),
    ("long", "Very long inputs, and conversations with many turns."),
    ("tricky", "Inputs written to trick it on purpose, such as hidden instructions inside the question."),
    ("fairness", "Fairness: whether names, gender or community change the answer."),
    ("drift", "How it behaves a month from now, after the model gets updated."),
    ("quick", "Most rewordings: this was a quick run with only a few variations."),
    ("repeat", "Asking the identical question several times."),
];

pub fn not_tested_codes(audit_types: &[AuditType], effort: super::Effort, groups: &[Group]) -> Vec<String> {
    let mut codes: Vec<&str> = vec![];
    if !groups.iter().any(|g| g.key == "language") {
        codes.push("language");
    }
    codes.extend(["long", "tricky", "fairness", "drift"]);
    if effort == super::Effort::Light {
        codes.insert(0, "quick");
    }
    if !audit_types.contains(&AuditType::Consistency) {
        codes.insert(0, "repeat");
    }
    codes.into_iter().take(5).map(String::from).collect()
}

pub fn not_tested(codes: &[String]) -> Vec<String> {
    codes.iter().filter_map(|c| NOT_TESTED.iter().find(|(k, _)| k == c).map(|(_, v)| v.to_string())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Verdict;

    fn test(id: &str, cat: &str, kind: TestKind, sim: f64) -> TestResult {
        let mut t = TestResult::new(id, cat, kind, "x", "in", "");
        t.output = Some("out".into());
        t.sim = Some(sim);
        t.verdict = Some(if sim >= 0.9 { Verdict::Same } else if sim >= 0.4 { Verdict::Partly } else { Verdict::Different });
        finish_test(&mut t);
        t
    }

    #[test]
    fn sensitivity_tests_score_the_other_way() {
        assert_eq!(test("a", "typo", TestKind::Invariance, 1.0).score, Some(1.0));
        assert_eq!(test("b", "control", TestKind::Sensitivity, 1.0).status, Status::Broke);
        assert_eq!(test("c", "control", TestKind::Sensitivity, 0.0).status, Status::Ok);
        assert_eq!(test("d", "typo", TestKind::Invariance, 0.5).status, Status::Wobble);
        let mut e = TestResult::new("e", "typo", TestKind::Invariance, "x", "in", "");
        finish_test(&mut e);
        assert_eq!(e.status, Status::Error);
    }

    #[test]
    fn weighted_score_and_grades() {
        let tests = vec![test("a", "typo", TestKind::Invariance, 1.0), test("b", "language", TestKind::Invariance, 0.0), test("c", "control", TestKind::Sensitivity, 0.0)];
        let (score, n, broke, skipped) = consistency(&tests);
        // weights 1.0, 0.7, 0.5 against scores 1, 0, 1 -> 1.5 / 2.2
        assert_eq!((score, n, broke, skipped), (0.68, 3, 1, 0));
        assert_eq!(grade_for(0.59), Grade::Concerning);
        assert_eq!(grade_for(0.6), Grade::Mixed);
        assert_eq!(grade_for(0.85), Grade::Mixed);
        assert_eq!(grade_for(0.86), Grade::Steady);
        assert_eq!(consistency(&[]).0, 0.0);
    }

    #[test]
    fn flags_follow_the_evidence() {
        let tests = vec![
            test("typo-1", "typo", TestKind::Invariance, 0.0),
            test("typo-2", "typo", TestKind::Invariance, 1.0),
            test("control-1", "control", TestKind::Sensitivity, 1.0),
            test("resample-1", "resample", TestKind::Resample, 0.2),
            test("resample-2", "resample", TestKind::Resample, 0.4),
        ];
        let faith = Faithfulness::skipped("");
        let flags = build_flags(&tests, &[], &faith, "ఇదిగో జవాబు", Lang::get("te").unwrap(), Some(false));
        let codes: Vec<&str> = flags.iter().map(|f| f.code.as_str()).collect();
        for c in ["surface_noise", "ignores_input", "unstable_sampling", "not_reproducible", "language_drift"] {
            assert!(codes.contains(&c), "{c} missing from {codes:?}");
        }
        assert!(flags.windows(2).all(|w| w[0].severity <= w[1].severity), "sorted worst first");
        assert_eq!(flags.iter().find(|f| f.code == "surface_noise").unwrap().severity, Severity::Medium);
    }

    #[test]
    fn next_steps_and_gaps() {
        assert_eq!(next_step_codes(&[]), vec!["default"]);
        assert_eq!(next_steps(&[])[0], DEFAULT_ADVICE);
        let codes = not_tested_codes(&[AuditType::Perturbation], super::super::Effort::Light, &[]);
        assert_eq!(&codes[..3], &["repeat", "quick", "language"]);
        assert_eq!(not_tested(&codes).len(), codes.len());
        assert_eq!(flag_codes().len(), ADVICE.len());
    }
}
