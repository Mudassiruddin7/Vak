//! Script-aware text helpers.
//!
//! The regex classes people reach for (`\w`, `[^\w\s]`) treat Indic vowel signs as punctuation, which
//! quietly chops Telugu, Hindi and the rest to pieces. Everything here goes by Unicode category instead.

mod json;
mod normalize;
mod script;
mod sentence;
mod similarity;

pub use json::{clip, extract_json, strip_think};
pub use normalize::{
    has_ascii_digits, has_native_digits, norm, numbers_in, squash, tidy_spaces, to_ascii_digits, to_native_digits,
    trim_punct, word_spans, Span,
};
pub use script::{dominant_script, major_class, script_counts, script_share, Script};
pub use sentence::{split_sentences, word_count};
pub use similarity::{lexical_similarity, lexical_verdict, settled_without_judge, Verdict};
