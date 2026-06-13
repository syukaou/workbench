use std::collections::HashMap;

use crate::engine::{Command, Engine};
use crate::error::Result;
use crate::event::Event;
use crate::EventStore;

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

    // ── State queries ──────────────────────────────────────────────

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

    /// Get the full event history (all events in the log, append-only).
    pub fn get_history(&self) -> Result<Vec<Event>> {
        self.engine.history()
    }

    // ── U1: Generic key-value ──────────────────────────────────────

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

    /// Execute a typed domain command directly.
    pub fn execute_command(&mut self, command: Command) -> Result<Event> {
        self.engine.execute(command)
    }

    // ── U2: Entity types and instances ─────────────────────────────

    /// Define a new entity type (e.g. "Boss", "Item").
    pub fn create_entity_type(&mut self, name: &str) -> Result<Event> {
        self.engine.execute(Command::CreateEntityType {
            name: name.to_string(),
        })
    }

    /// Create an instance of an entity type.
    pub fn create_entity_instance(
        &mut self,
        entity_type: &str,
        instance_id: &str,
    ) -> Result<Event> {
        self.engine.execute(Command::CreateEntityInstance {
            entity_type: entity_type.to_string(),
            instance_id: instance_id.to_string(),
        })
    }

    /// Set a field value on an entity instance.
    pub fn set_entity_field(
        &mut self,
        instance_id: &str,
        field: &str,
        value: serde_json::Value,
    ) -> Result<Event> {
        self.engine.execute(Command::SetEntityField {
            instance_id: instance_id.to_string(),
            field: field.to_string(),
            value,
        })
    }

    // ── U3: Graph topology ─────────────────────────────────────────

    /// Add a node (room/area) to the graph.
    pub fn create_node(&mut self, node_id: &str, label: &str) -> Result<Event> {
        self.engine.execute(Command::CreateNode {
            node_id: node_id.to_string(),
            label: label.to_string(),
        })
    }

    /// Remove a node from the graph.
    pub fn remove_node(&mut self, node_id: &str) -> Result<Event> {
        self.engine.execute(Command::RemoveNode {
            node_id: node_id.to_string(),
        })
    }

    /// Add an edge between two nodes.
    pub fn create_edge(
        &mut self,
        from_node: &str,
        to_node: &str,
        bidirectional: bool,
    ) -> Result<Event> {
        self.engine.execute(Command::CreateEdge {
            from_node: from_node.to_string(),
            to_node: to_node.to_string(),
            bidirectional,
        })
    }

    /// Remove an edge.
    pub fn remove_edge(&mut self, from_node: &str, to_node: &str) -> Result<Event> {
        self.engine.execute(Command::RemoveEdge {
            from_node: from_node.to_string(),
            to_node: to_node.to_string(),
        })
    }

    /// Apply a semantic mark to a node (e.g. "spawn", "shortcut").
    pub fn mark_node(&mut self, node_id: &str, mark: &str) -> Result<Event> {
        self.engine.execute(Command::MarkNode {
            node_id: node_id.to_string(),
            mark: mark.to_string(),
        })
    }

    // ── U3: POI ────────────────────────────────────────────────────

    /// Attach a POI to a node, optionally referencing an entity instance.
    pub fn attach_poi(
        &mut self,
        node_id: &str,
        poi_id: &str,
        entity_ref: Option<&str>,
    ) -> Result<Event> {
        self.engine.execute(Command::AttachPOI {
            node_id: node_id.to_string(),
            poi_id: poi_id.to_string(),
            entity_ref: entity_ref.map(|s| s.to_string()),
        })
    }

    /// Detach a POI from a node.
    pub fn detach_poi(&mut self, node_id: &str, poi_id: &str) -> Result<Event> {
        self.engine.execute(Command::DetachPOI {
            node_id: node_id.to_string(),
            poi_id: poi_id.to_string(),
        })
    }

    // ── Undo / Redo ────────────────────────────────────────────────

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

    // ── Replay / INV-5 verification ────────────────────────────────

    /// Rebuild state from the event log — full replay from event seq 0.
    /// Used to verify INV-5: state = fold(events).
    pub fn rebuild(&self) -> Result<HashMap<String, serde_json::Value>> {
        self.engine.rebuild()
    }

    /// Rebuild state up to a specific sequence number.
    pub fn rebuild_up_to(&self, seq: u64) -> Result<HashMap<String, serde_json::Value>> {
        self.engine.rebuild_up_to(seq)
    }

    // ── v1.4: Save/Load persistence ─────────────────────────────────

    /// Export a full project snapshot: all events + materialized state.
    pub fn export_snapshot(&self) -> Result<serde_json::Value> {
        let events = self.engine.history()?;
        let state = self.engine.state().clone();
        Ok(serde_json::json!({
            "version": 1,
            "events": events,
            "state": state,
        }))
    }

    /// Import a project snapshot, replacing the current state entirely.
    pub fn import_snapshot(&mut self, events: &[Event]) -> Result<()> {
        self.engine.import_events(events)
    }
}
