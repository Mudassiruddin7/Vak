//! Vak's domain layer: everything that can be decided without touching the network.
//!
//! * [`text`]  Unicode and Indic-script handling (normalising, comparing, splitting).
//! * [`lang`]  the language registry, writing rules and the checker that enforces them.
//! * [`audit`] perturbations, scoring, flags, effort planning, practice models.
//! * [`live`]  guards and the trust score for the live interpreter.
//!
//! Nothing in here does I/O, so every function is deterministic and unit-testable.

pub mod audit;
pub mod lang;
pub mod live;
pub mod rng;
pub mod text;


