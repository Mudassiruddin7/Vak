use std::collections::HashMap;
use std::time::Duration;

use vak_app::engine::Settings;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub sarvam_key: String,
    pub sarvam_base: String,
    pub rpm: u32,
    pub supabase_url: String,
    pub supabase_key: String,
    pub allow_custom: bool,
    pub allow_private_targets: bool,
    pub trust_proxy: bool,
    pub signing_key: Vec<u8>,
    pub anon_units_per_day: f64,
    pub user_units_per_day: f64,
    pub max_concurrent_audits: usize,
    pub audit_cost_cap_inr: f64,
    pub settings: Settings,
}

/// `KEY=value` lines; spaces around `=`, `export`, and quotes are tolerated. Existing environment wins.
pub fn load_dotenv(path: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    for line in text.lines() {
        let line = line.trim().trim_start_matches("export ").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            out.insert(k.trim().to_string(), v.to_string());
        }
    }
    out
}

impl Config {
    pub fn from_env() -> Config {
        let file = load_dotenv(".env");
        let get = |names: &[&str]| -> String {
            for n in names {
                if let Ok(v) = std::env::var(n) {
                    if !v.is_empty() {
                        return v;
                    }
                }
                if let Some(v) = file.get(*n).filter(|v| !v.is_empty()) {
                    return v.clone();
                }
            }
            String::new()
        };
        let or = |names: &[&str], d: &str| {
            let v = get(names);
            if v.is_empty() { d.to_string() } else { v }
        };
        let num = |names: &[&str], d: f64| get(names).parse::<f64>().unwrap_or(d);
        let flag = |names: &[&str]| matches!(get(names).to_lowercase().as_str(), "1" | "true" | "yes");
        let key = get(&["SARVAM_API_KEY"]);
        let mut settings = Settings::default();
        settings.plan_model = or(&["VAK_PLAN_MODEL"], &settings.plan_model);
        settings.judge_model = or(&["VAK_JUDGE_MODEL"], &settings.judge_model);
        settings.explain_model = or(&["VAK_EXPLAIN_MODEL"], &settings.explain_model);
        settings.target_model = or(&["VAK_TARGET_MODEL"], &settings.target_model);
        settings.parallel = num(&["VAK_PARALLEL_CALLS"], 6.0) as usize;
        settings.max_prompt_chars = num(&["VAK_MAX_PROMPT_CHARS"], 600.0) as usize;
        let signing = get(&["VAK_SIGNING_KEY"]);
        let signing_key = if signing.is_empty() { format!("vak-report-signing:{key}").into_bytes() } else { signing.into_bytes() };
        let hosted = flag(&["VAK_HOSTED"]);
        Config {
            host: or(&["VAK_HOST"], if hosted { "0.0.0.0" } else { "127.0.0.1" }),
            port: get(&["PORT", "VAK_PORT"]).parse().unwrap_or(8420),
            sarvam_key: key,
            sarvam_base: or(&["SARVAM_BASE_URL"], "https://api.sarvam.ai"),
            rpm: num(&["VAK_RPM"], 48.0) as u32,
            supabase_url: get(&["SUPABASE_URL", "NEXT_PUBLIC_SUPABASE_URL"]),
            supabase_key: get(&["SUPABASE_PUBLISHABLE_KEY", "NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY"]),
            // a public deployment must not let strangers aim the server at private addresses
            allow_custom: if hosted { flag(&["VAK_ALLOW_CUSTOM"]) } else { true },
            allow_private_targets: !hosted && !flag(&["VAK_BLOCK_PRIVATE"]),
            trust_proxy: hosted || flag(&["VAK_TRUST_PROXY"]),
            signing_key,
            anon_units_per_day: num(&["VAK_ANON_UNITS_PER_DAY"], 24.0),
            user_units_per_day: num(&["VAK_USER_UNITS_PER_DAY"], 80.0),
            max_concurrent_audits: num(&["VAK_MAX_CONCURRENT_AUDITS"], 4.0) as usize,
            audit_cost_cap_inr: num(&["VAK_AUDIT_COST_CAP_INR"], 15.0),
            settings: Settings { call_timeout: Duration::from_secs(120), run_timeout: Duration::from_secs(280), ..settings },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_tolerates_spaces_quotes_and_comments() {
        let dir = std::env::temp_dir().join(format!("vak-env-{}", std::process::id()));
        std::fs::write(&dir, "# c\nSARVAM_API_KEY = abc123\nexport A=\"x y\"\n\nB='z'\n").unwrap();
        let m = load_dotenv(dir.to_str().unwrap());
        assert_eq!((m["SARVAM_API_KEY"].as_str(), m["A"].as_str(), m["B"].as_str()), ("abc123", "x y", "z"));
        assert!(load_dotenv("/nonexistent/.env").is_empty());
    }
}
