//! Everything that depends on which language a person reads or speaks: the registry of 23 languages,
//! the wording banks, the writing-rule checker and the prompt that asks a model to follow those rules.

mod fallback;
mod prompt;
mod registry;
mod rules;
mod texts;

pub use fallback::build as fallback_explanation;
pub use prompt::{build_system, build_user};
pub use registry::{detect, Lang, LangInfo, Tier, Voices};
pub use rules::{check, clean, split_long, Facts, Violations, MAX_SENTENCES, MIN_SENTENCES};
pub use texts::{text_for, Bank, LangText, Notices, PoliteVariant, VariantText};
