//! workbench-core: Deterministic event-log core.
//!
//! This crate provides the single source of truth for all design state.
//! All mutations flow through Command → validate → serial append to an
//! append-only typed event log. State is derived by folding events.
//! The core crate has zero LLM/HTTP/rendering dependencies (INV-4/7).

pub mod error;
pub mod event;
pub mod projection;
pub mod engine;
pub mod contract;

// Event store: SQLite when native, in-memory Vec when WASM.
#[cfg(feature = "native")]
pub mod log;
#[cfg(feature = "native")]
pub use log::EventStore;

#[cfg(not(feature = "native"))]
pub mod memory_store;
#[cfg(not(feature = "native"))]
pub use memory_store::MemoryStore as EventStore;

// CLI bridge: only available on native (uses std::process).
#[cfg(feature = "native")]
pub mod cli_bridge;

// WASM IPC: only available on wasm32 target.
#[cfg(target_arch = "wasm32")]
pub mod wasm;

// Re-export the public API.
pub use contract::WorkbenchCore;
pub use error::{Error, Result};
pub use event::{Event, EventType};
pub use projection::{HashMapProjection, Projection};

#[cfg(test)]
mod invariant_tests;
