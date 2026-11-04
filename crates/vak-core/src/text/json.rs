use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

static THINK_BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<think>.*?</think>").expect("static regex"));
static THINK_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<think>.*\z").expect("static regex"));
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)```(?:json)?\s*(.*?)```").expect("static regex"));

/// Drop `<think>...</think>` blocks, including one that was cut off by a length limit.
pub fn strip_think(text: &str) -> String {
    let t = THINK_BLOCK.replace_all(text, "");
    THINK_OPEN.replace_all(&t, "").trim().to_string()
}

/// Pull the first JSON object or array out of model output.
pub fn extract_json(text: &str) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    let mut candidates = vec![text.trim().to_string()];
    candidates.extend(FENCE.captures_iter(text).map(|c| c[1].trim().to_string()));
    for cand in &candidates {
        if let Ok(v) = serde_json::from_str::<Value>(cand) {
            return Some(v);
        }
    }
    for (open, close) in [('{', '}'), ('[', ']')] {
        let chars: Vec<char> = text.chars().collect();
        let mut from = 0;
        while let Some(start) = chars[from..].iter().position(|&c| c == open).map(|p| p + from) {
            if let Some(end) = balanced_end(&chars, start, open, close) {
                let slice: String = chars[start..=end].iter().collect();
                if let Ok(v) = serde_json::from_str::<Value>(&slice) {
                    return Some(v);
                }
            }
            from = start + 1;
        }
    }
    None
}

fn balanced_end(chars: &[char], start: usize, open: char, close: char) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &ch) in chars.iter().enumerate().skip(start) {
        if in_str {
            match (esc, ch) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        if ch == '"' {
            in_str = true;
        } else if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Shorten to at most `limit` characters, ending with an ellipsis when something was cut.
pub fn clip(text: &str, limit: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= limit {
        return t.to_string();
    }
    let head: String = t.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", head.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_is_found_in_noise() {
        assert_eq!(extract_json(r#"{"a": 1}"#), Some(json!({"a": 1})));
        assert_eq!(extract_json("Sure!\n```json\n{\"a\": [1, 2]}\n```"), Some(json!({"a": [1, 2]})));
        assert_eq!(extract_json(r#"prefix {"a": "}"} suffix"#), Some(json!({"a": "}"})));
        assert_eq!(extract_json("nothing here"), None);
        assert_eq!(extract_json(""), None);
    }

    #[test]
    fn think_blocks_go() {
        assert_eq!(strip_think("<think>a</think>hello"), "hello");
        assert_eq!(strip_think("hello<think>cut off mid-thought"), "hello");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("  ab ", 4), "ab");
    }
}
