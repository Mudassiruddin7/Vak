//! Request, plan and report shapes. These are what cross the API boundary, and their JSON matches the
//! shapes the browser already consumes.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditType {
    Consistency,
    Perturbation,
    Behavioral,
}

pub const ALL_TYPES: [AuditType; 3] = [AuditType::Consistency, AuditType::Perturbation, AuditType::Behavioral];

impl AuditType {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditType::Consistency => "consistency",
            AuditType::Perturbation => "perturbation",
            AuditType::Behavioral => "behavioral",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestKind {
    Invariance,
    Sensitivity,
    Resample,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Wobble,
    Broke,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Concerning,
    Mixed,
    Steady,
}

/// One edit of the prompt that should (invariance) or should not (sensitivity) change the answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub category: String,
    pub kind: TestKind,
    pub label: String,
    pub text: String,
    #[serde(default)]
    pub note: String,
}

impl Variant {
    pub fn new(id: impl Into<String>, category: &str, kind: TestKind, label: impl Into<String>, text: String) -> Self {
        Variant { id: id.into(), category: category.into(), kind, label: label.into(), text, note: String::new() }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestResult {
    pub id: String,
    pub category: String,
    pub kind: TestKind,
    pub label: String,
    pub input: String,
    pub output: Option<String>,
    pub sim: Option<f64>,
    pub verdict: Option<crate::text::Verdict>,
    pub score: Option<f64>,
    pub status: Status,
    pub judged: bool,
    pub reason: String,
    pub note: String,
}

impl TestResult {
    pub fn new(id: &str, category: &str, kind: TestKind, label: &str, input: &str, note: &str) -> Self {
        TestResult {
            id: id.into(),
            category: category.into(),
            kind,
            label: label.into(),
            input: input.into(),
            output: None,
            sim: None,
            verdict: None,
            score: None,
            status: Status::Error,
            judged: false,
            reason: String::new(),
            note: note.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub index: usize,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub importance: f64,
    pub raw: f64,
    pub changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flag {
    pub code: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FaithStatus {
    Ok,
    Inconclusive,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Faithfulness {
    pub status: FaithStatus,
    pub score: Option<f64>,
    pub claimed: Vec<String>,
    pub matched: Vec<usize>,
    pub top: Vec<usize>,
    pub note: String,
}

impl Faithfulness {
    pub fn skipped(note: &str) -> Self {
        Faithfulness { status: FaithStatus::Skipped, score: None, claimed: vec![], matched: vec![], top: vec![], note: note.into() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub key: String,
    pub label: String,
    pub score: f64,
    pub n: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub prompt: String,
    pub prompt_language: String,
    pub baseline_output: String,
    pub reference_output: Option<String>,
    pub target_label: String,
    pub depth: String,
    pub effort: String,
    pub audit_types: Vec<AuditType>,
    pub consistency_score: f64,
    pub grade: Grade,
    pub total_perturbations: usize,
    pub unexpected_changes: usize,
    pub skipped: usize,
    pub tests: Vec<TestResult>,
    pub groups: Vec<Group>,
    pub units: Vec<Unit>,
    pub faithfulness: Faithfulness,
    pub flags: Vec<Flag>,
    pub strengths: Vec<String>,
    pub next_steps: Vec<String>,
    pub next_step_codes: Vec<String>,
    pub not_tested: Vec<String>,
    pub not_tested_codes: Vec<String>,
    pub notes: Vec<String>,
    pub judge_used: bool,
    pub cost_inr: f64,
    pub seconds: f64,
    /// How many calls the audited model, the judge and the helpers actually made, so the saving from
    /// adaptive testing is visible rather than claimed.
    pub calls: CallStats,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CallStats {
    pub target: usize,
    pub helpers: usize,
    pub cached: usize,
    pub planned: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
}
