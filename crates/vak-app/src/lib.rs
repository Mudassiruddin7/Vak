//! Use cases: the audit engine, the explainer, the planner and the live interpreter. Everything that
//! touches the outside world goes through the traits in [`ports`], so all of it runs against fakes in tests.

pub mod catalog;
pub mod engine;
pub mod explain;
pub mod judge;
pub mod live;
pub mod orchestrator;
pub mod plan;
pub mod ports;
pub mod target;

#[cfg(test)]
pub mod fakes;
