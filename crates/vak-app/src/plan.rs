use serde::{Deserialize, Serialize};
use vak_core::audit::{AuditType, Effort, ALL_TYPES};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetSpec {
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// A custom OpenAI-compatible endpoint. The key lives in memory for the run and is never stored.
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub remote_model: Option<String>,
}

fn default_kind() -> String {
    "sarvam".into()
}
fn default_model() -> String {
    "sarvam-105b-conversations".into()
}

impl Default for TargetSpec {
    fn default() -> Self {
        TargetSpec { kind: default_kind(), model: default_model(), system_prompt: None, base_url: None, api_key: None, remote_model: None }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub prompt: String,
    #[serde(default)]
    pub reference_output: Option<String>,
    #[serde(default)]
    pub target: TargetSpec,
    #[serde(default = "all_types")]
    pub audit_types: Vec<AuditType>,
    #[serde(default = "balanced", alias = "depth")]
    pub effort: Effort,
    #[serde(default = "te")]
    pub prompt_language: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub source_text: String,
}

fn all_types() -> Vec<AuditType> {
    ALL_TYPES.to_vec()
}
fn balanced() -> Effort {
    Effort::Balanced
}
fn te() -> String {
    "te".into()
}

impl Plan {
    /// Trim, de-duplicate and default what a client sent.
    pub fn normalised(mut self) -> Result<Plan, String> {
        self.prompt = self.prompt.trim().to_string();
        if self.prompt.is_empty() {
            return Err("The prompt to test can't be empty.".into());
        }
        let mut seen = vec![];
        for t in std::mem::take(&mut self.audit_types) {
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        self.audit_types = if seen.is_empty() { ALL_TYPES.to_vec() } else { seen };
        self.target.system_prompt = self.target.system_prompt.take().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if self.target.system_prompt.as_ref().is_some_and(|s| s.chars().count() > 1500) {
            return Err("The system prompt is too long (1500 characters at most).".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditRequest {
    pub plan: Plan,
    #[serde(default = "te")]
    pub language: String,
    #[serde(default)]
    pub voice: bool,
    #[serde(default)]
    pub voice_id: Option<String>,
    #[serde(default)]
    pub fresh: bool,
}
