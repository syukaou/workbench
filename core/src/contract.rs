use std::collections::HashMap;

use crate::engine::{Command, Engine};
use crate::error::Result;
use crate::event::Event;
use crate::log::EventStore;

/// The single typed contract boundary for the core (INV-6).
///
/// All consumers — frontend, Tauri shell, future integrations — must interact
/// with the core exclusively through this API. No consumer may reach inside
/// core modules directly.
pub struct WorkbenchCore {
    engine: Engine,
}

impl WorkbenchCore {
    /// Create a new WorkbenchCore backed by a file-based event store.
    ///
    /// The store path should be inside the project directory (e.g., `project_dir/events.db`).
    pub fn open(db_path: &str, aggregate_id: impl Into<String>) -> Result<Self> {
        let store = EventStore::open(db_path)?;
        let engine = Engine::new(store, aggregate_id)?;
        Ok(WorkbenchCore { engine })
    }

    /// Create a new WorkbenchCore with an in-memory store (for testing).
    pub fn open_in_memory(aggregate_id: impl Into<String>) -> Result<Self> {
        let store = EventStore::open_in_memory()?;
        let engine = Engine::new(store, aggregate_id)?;
        Ok(WorkbenchCore { engine })
    }

    /// Get a snapshot of the current materialized state.
    pub fn get_state(&self) -> HashMap<String, serde_json::Value> {
        self.engine.state().clone()
    }

    /// Get the current sequence number (undo/redo cursor position).
    pub fn get_current_seq(&self) -> u64 {
        self.engine.current_seq()
    }

    /// Get the total number of events in the log.
    pub fn get_total_events(&self) -> Result<u64> {
        self.engine.total_events()
    }

    /// Execute a command: validate → serial append to event log → update state.
    ///
    /// This is the **sole write path** for all data (INV-2).
    pub fn execute(&mut self, key: &str, value: Option<serde_json::Value>) -> Result<Event> {
        let command = match value {
            Some(v) => Command::Set {
                key: key.to_string(),
                value: v,
            },
            None => Command::Delete {
                key: key.to_string(),
            },
        };
        self.engine.execute(command)
    }

    /// Set a key to a value. Convenience wrapper around `execute`.
    pub fn set(&mut self, key: &str, value: serde_json::Value) -> Result<Event> {
        self.execute(key, Some(value))
    }

    /// Delete a key. Convenience wrapper around `execute`.
    pub fn delete(&mut self, key: &str) -> Result<Event> {
        self.execute(key, None)
    }

    /// Undo `count` events. Returns the number actually undone.
    pub fn undo(&mut self, count: u32) -> Result<u32> {
        self.engine.undo(count)
    }

    /// Undo all events back to seq 0.
    pub fn undo_all(&mut self) -> Result<u32> {
        self.engine.undo_all()
    }

    /// Redo `count` events forward. Returns the number actually redone.
    pub fn redo(&mut self, count: u32) -> Result<u32> {
        self.engine.redo(count)
    }

    /// Redo all remaining events.
    pub fn redo_all(&mut self) -> Result<u32> {
        self.engine.redo_all()
    }

    /// Get the full event history (all events in the log, append-only).
    pub fn get_history(&self) -> Result<Vec<Event>> {
        self.engine.history()
    }

    /// Rebuild state from the event log — full replay from event seq 0.
    /// Used to verify INV-5: state = fold(events).
    pub fn rebuild(&self) -> Result<HashMap<String, serde_json::Value>> {
        self.engine.rebuild()
    }

    /// Rebuild state up to a specific sequence number.
    pub fn rebuild_up_to(&self, seq: u64) -> Result<HashMap<String, serde_json::Value>> {
        self.engine.rebuild_up_to(seq)
    }
}
