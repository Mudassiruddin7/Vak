//! The explanation of last resort: assembled from the pre-written bank, so someone always gets something
//! readable even when the model cannot keep to the rules.

use super::texts::text_for;
use super::Lang;
use crate::audit::scoring::{CONCERN_BELOW, STEADY_ABOVE};
use crate::audit::Report;
use crate::text::trim_punct;

const MAX_FLAGS: usize = 3;
const MAX_STEPS: usize = 3;

pub fn build(report: &Report, lang: Lang) -> String {
    let t = &text_for(&lang.code).or_else(|| text_for("en")).expect("english wording exists").bank;
    let score = report.consistency_score;
    let mut lines: Vec<String> = vec![t.model.clone(), t.what.clone(), t.total.replace("{n}", &report.total_perturbations.to_string())];
    lines.push(if report.unexpected_changes > 0 { t.changed.replace("{n}", &report.unexpected_changes.to_string()) } else { t.unchanged.clone() });
    lines.push(t.score.replace("{n}", &((score * 100.0).round() as i64).to_string()));
    let tone = if score < CONCERN_BELOW { &t.concern } else if score > STEADY_ABOVE { &t.steady } else { &t.mixed };
    lines.extend(tone.iter().cloned());

    let codes: Vec<&str> = report.flags.iter().map(|f| f.code.as_str()).collect();
    lines.extend(codes.iter().take(MAX_FLAGS).filter_map(|c| t.flags.get(*c).cloned()));

    let leaders: Vec<_> = report.units.iter().filter(|u| u.raw >= 0.5 && u.importance >= 0.99).collect();
    if leaders.len() == 1 && report.units.len() >= 4 {
        let word = trim_punct(&leaders[0].text);
        if !word.is_empty() {
            lines.push(t.top_word.replace("{w}", word));
        }
    }
    lines.push(t.next.clone());
    let steps: Vec<String> = codes.iter().take(MAX_STEPS).filter_map(|c| t.steps.get(*c).cloned()).collect();
    if steps.is_empty() {
        lines.push(t.steps["default"].clone());
    } else {
        lines.extend(steps);
    }
    lines.join(" ")
}
