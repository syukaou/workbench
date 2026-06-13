//! workbench-core: Deterministic event-log core.
//!
//! This crate provides the single source of truth for all design state.
//! All mutations flow through Command → validate → serial append to an
//! append-only typed event log. State is derived by folding events.
//! The core crate has zero LLM/HTTP/rendering dependencies (INV-4/7).

pub mod error;
pub mod event;
pub mod log;
pub mod projection;
pub mod engine;
pub mod contract;

// Re-export the public API.
pub use contract::WorkbenchCore;
pub use error::{Error, Result};
pub use event::{Event, EventType};
pub use projection::{HashMapProjection, Projection};

#[cfg(test)]
mod invariant_tests;
