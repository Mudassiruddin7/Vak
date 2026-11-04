//! The audit itself: what gets changed in a prompt, how answers are compared and scored, and how much
//! work a given effort level buys. Pure functions only; the engine in `vak-app` does the asking.

pub mod effort;
pub mod faithfulness;
pub mod importance;
pub mod perturb;
pub mod practice;
pub mod scoring;
pub mod types;

pub use effort::{difficulty, Difficulty, Effort, Preset};
pub use types::*;
