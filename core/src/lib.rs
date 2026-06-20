//! workbench-core: Deterministic event-log core.
//!
//! This crate provides the single source of truth for all design state.
//! All mutations flow through Command → validate → serial append to an
//! append-only typed event log. State is derived by folding events.
//! The core crate has zero LLM/HTTP/rendering dependencies (INV-4/7).

pub mod contract;
pub mod error;
pub mod event;
pub mod projection;

// INV-6 (single contract boundary): the deterministic core's internals — the
// Engine and the event store — are NOT part of the public API. A consumer must
// never construct an Engine or EventStore directly and bypass WorkbenchCore.
// `engine` is crate-private; only the contract type and the typed `Command`
// surface are re-exported below.
mod engine;

// Event store: SQLite when native, in-memory Vec when WASM. Crate-private —
// reachable internally as `crate::EventStore`, never exported (INV-6).
#[cfg(feature = "native")]
mod log;
#[cfg(feature = "native")]
use log::EventStore;

#[cfg(not(feature = "native"))]
mod memory_store;
#[cfg(not(feature = "native"))]
use memory_store::MemoryStore as EventStore;

// CLI bridge: only available on native (uses std::process). This is the AI
// proposal layer — an *outside* layer per the architecture, not the core.
#[cfg(feature = "native")]
pub mod cli_bridge;

// CLI HTTP server: only available on native (uses std::net).
#[cfg(feature = "native")]
pub mod cli_server;

// WASM IPC: only available on wasm32 target.
#[cfg(target_arch = "wasm32")]
pub mod wasm;

// Re-export the public API. This is the entire INV-6 contract surface:
// WorkbenchCore (the sole boundary) plus the typed values consumers exchange
// with it — Command (for execute_command), Event/EventType, errors, and the
// read-only Projection trait. engine/log/memory_store stay private.
pub use contract::WorkbenchCore;
pub use engine::Command;
pub use error::{Error, Result};
pub use event::{Event, EventType};
pub use projection::{HashMapProjection, Projection};

#[cfg(test)]
mod invariant_tests;
