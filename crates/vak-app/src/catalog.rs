//! Models and prices. Prices are Sarvam's pay-as-you-go list prices in rupees, read off the pricing page
//! on 4 Nov 2026. They only drive the budget meter, so if they drift the worst case is a wrong figure on screen.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ChatModel {
    pub id: &'static str,
    pub label: &'static str,
    /// "v1" or "v2": the path prefix on the API.
    pub api: &'static str,
    pub inr_in: f64,
    pub inr_out: f64,
    pub blurb: &'static str,
}

pub const CHAT_MODELS: [ChatModel; 4] = [
    ChatModel { id: "sarvam-105b-conversations", label: "Sarvam 105B (conversational)", api: "v1", inr_in: 15.0, inr_out: 60.0, blurb: "Tuned for everyday dialogue. The default target." },
    ChatModel { id: "sarvam-105b", label: "Sarvam 105B", api: "v1", inr_in: 15.0, inr_out: 60.0, blurb: "The flagship, with its thinking mode switched off." },
    ChatModel { id: "gemma4", label: "Gemma 4 31B", api: "v2", inr_in: 35.0, inr_out: 90.0, blurb: "Open-weight model served through Sarvam." },
    ChatModel { id: "deepseekv4.1-flash-1017", label: "DeepSeek V4.1 Flash", api: "v2", inr_in: 25.0, inr_out: 105.0, blurb: "Open-weight model served through Sarvam." },
];

pub fn chat_model(id: &str) -> Option<&'static ChatModel> {
    CHAT_MODELS.iter().find(|m| m.id == id)
}

pub fn chat_api_for(id: &str) -> &'static str {
    chat_model(id).map(|m| m.api).unwrap_or(if id.starts_with("sarvam-105b") { "v1" } else { "v2" })
}

/// Unknown models are charged at the flagship rate so the meter errs on the high side.
pub fn chat_cost_inr(id: &str, tokens_in: u64, tokens_out: u64) -> f64 {
    let m = chat_model(id).unwrap_or(&CHAT_MODELS[1]);
    (tokens_in as f64 * m.inr_in + tokens_out as f64 * m.inr_out) / 1_000_000.0
}

pub const STT_INR_PER_MIN: f64 = 0.49;
pub const TRANSLATE_INR_PER_1K_CHARS: f64 = 2.0;

pub fn tts_cost_inr(model: &str, chars: usize) -> f64 {
    (if model == "bulbul:v4-flash" { 1.0 } else { 3.0 }) * chars as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_add_up() {
        assert!((chat_cost_inr("sarvam-105b", 1_000_000, 1_000_000) - 75.0).abs() < 1e-9);
        assert_eq!(chat_cost_inr("mystery", 0, 0), 0.0);
        assert_eq!(chat_api_for("gemma4"), "v2");
        assert_eq!(chat_api_for("sarvam-105b-x"), "v1");
        assert!((tts_cost_inr("bulbul:v3", 2000) - 6.0).abs() < 1e-9);
    }
}
