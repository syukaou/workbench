use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::event::{Event, EventType};
use crate::log::EventStore;
use crate::projection::{HashMapProjection, Projection};

/// A command to mutate the system state. Every command goes through
/// validate → serialize → append to event log → update state (INV-2).
#[derive(Debug, Clone)]
pub enum Command {
    // ── U1: Generic ──
    /// Set a key to a value.
    Set {
        key: String,
        value: serde_json::Value,
    },
    /// Delete a key.
    Delete {
        key: String,
    },

    // ── U2: Entities ──
    /// Define a new entity type.
    CreateEntityType {
        name: String,
    },
    /// Create an instance of an entity type.
    CreateEntityInstance {
        entity_type: String,
        instance_id: String,
    },
    /// Set a field value on an entity instance.
    SetEntityField {
        instance_id: String,
        field: String,
        value: serde_json::Value,
    },

    // ── U3: Graph topology ──
    /// Add a node (room/area) to the graph.
    CreateNode {
        node_id: String,
        label: String,
    },
    /// Remove a node from the graph.
    RemoveNode {
        node_id: String,
    },
    /// Add an edge between two nodes.
    CreateEdge {
        from_node: String,
        to_node: String,
        bidirectional: bool,
    },
    /// Remove an edge.
    RemoveEdge {
        from_node: String,
        to_node: String,
    },
    /// Apply a semantic mark to a node.
    MarkNode {
        node_id: String,
        mark: String,
    },

    // ── U3: POI ──
    /// Attach a POI to a node, optionally referencing an entity instance.
    AttachPOI {
        node_id: String,
        poi_id: String,
        entity_ref: Option<String>,
    },
    /// Detach a POI from a node.
    DetachPOI {
        node_id: String,
        poi_id: String,
    },
}

/// The core engine: processes commands through the event log,
/// maintains current materialized state, and supports undo/redo.
///
/// INV-2: all mutations go through Command → validate → serial append to event log.
/// INV-5: events are append-only; state = fold(events); undo = move cursor, re-fold.
pub struct Engine {
    store: EventStore,
    aggregate_id: String,
    /// The latest sequence number applied to the current state.
    /// This is the undo/redo cursor position.
    current_seq: u64,
    /// The materialized state at `current_seq`, maintained by a HashMapProjection.
    state: HashMapProjection,
}

impl Engine {
    /// Create a new engine backed by the given EventStore.
    /// The state is initialized by folding all existing events.
    pub fn new(store: EventStore, aggregate_id: impl Into<String>) -> Result<Self> {
        let aggregate_id = aggregate_id.into();
        let current_seq = store.event_count(&aggregate_id)?;
        let state = fold_projection(&store, &aggregate_id, current_seq)?;

        Ok(Engine {
            store,
            aggregate_id,
            current_seq,
            state,
        })
    }

    /// Get a snapshot of the current materialized state.
    pub fn state(&self) -> &HashMap<String, serde_json::Value> {
        self.state.as_map()
    }

    /// Get the current sequence number (the undo/redo cursor position).
    pub fn current_seq(&self) -> u64 {
        self.current_seq
    }

    /// Get the total number of events in the log.
    pub fn total_events(&self) -> Result<u64> {
        self.store.event_count(&self.aggregate_id)
    }

    /// Execute a command: validate → append event → update state.
    ///
    /// Returns the persisted event. The state is immediately updated.
    /// This is the **sole write path** (INV-2).
    pub fn execute(&mut self, command: Command) -> Result<Event> {
        // If the user has undone some events, any new command truncates the "future"
        // (the undone events are still in the log, but the new event effectively
        // forks from the current_seq). The undone events become unreachable from
        // the new forward timeline, but remain in the log for audit (INV-5).
        self.current_seq = self.store.event_count(&self.aggregate_id)?;

        let (event_type, payload) = match &command {
            Command::Set { key, value } => (
                EventType::Set,
                serde_json::json!({"key": key, "value": value}),
            ),
            Command::Delete { key } => (
                EventType::Delete,
                serde_json::json!({"key": key}),
            ),
            Command::CreateEntityType { name } => (
                EventType::EntityTypeCreated,
                serde_json::json!({"name": name}),
            ),
            Command::CreateEntityInstance {
                entity_type,
                instance_id,
            } => (
                EventType::EntityInstanceCreated,
                serde_json::json!({"entity_type": entity_type, "instance_id": instance_id}),
            ),
            Command::SetEntityField {
                instance_id,
                field,
                value,
            } => (
                EventType::EntityInstanceFieldSet,
                serde_json::json!({"instance_id": instance_id, "field": field, "value": value}),
            ),
            Command::CreateNode { node_id, label } => (
                EventType::NodeCreated,
                serde_json::json!({"node_id": node_id, "label": label}),
            ),
            Command::RemoveNode { node_id } => (
                EventType::NodeRemoved,
                serde_json::json!({"node_id": node_id}),
            ),
            Command::CreateEdge {
                from_node,
                to_node,
                bidirectional,
            } => (
                EventType::EdgeCreated,
                serde_json::json!({"from": from_node, "to": to_node, "bidirectional": bidirectional}),
            ),
            Command::RemoveEdge {
                from_node,
                to_node,
            } => (
                EventType::EdgeRemoved,
                serde_json::json!({"from": from_node, "to": to_node}),
            ),
            Command::MarkNode { node_id, mark } => (
                EventType::NodeMarked,
                serde_json::json!({"node_id": node_id, "mark": mark}),
            ),
            Command::AttachPOI {
                node_id,
                poi_id,
                entity_ref,
            } => (
                EventType::POIAttached,
                serde_json::json!({"node_id": node_id, "poi_id": poi_id, "entity_ref": entity_ref}),
            ),
            Command::DetachPOI { node_id, poi_id } => (
                EventType::POIDetached,
                serde_json::json!({"node_id": node_id, "poi_id": poi_id}),
            ),
        };

        // Validate: check that the payload is valid JSON.
        serde_json::to_string(&payload).map_err(|e| Error::InvalidCommand(format!("Cannot serialize payload: {}", e)))?;

        let next_seq = self.current_seq + 1;
        let timestamp = timestamp_ms();

        let event = Event::new(next_seq, &self.aggregate_id, event_type, payload, timestamp);
        let persisted = self.store.append(&event)?;

        // Apply the event to the current state via projection.
        self.state.apply_event(&persisted);
        self.current_seq = persisted.seq;

        Ok(persisted)
    }

    /// Undo `count` events from the current position, moving backward in history.
    /// Returns the number of events actually undone.
    ///
    /// Events are never deleted — we simply move the cursor back and re-fold
    /// the state from events 0..current_seq-count (INV-5).
    pub fn undo(&mut self, count: u32) -> Result<u32> {
        if self.current_seq == 0 {
            return Err(Error::NothingToUndo);
        }

        let target_seq = self.current_seq.saturating_sub(count as u64);
        let undone = (self.current_seq - target_seq) as u32;

        self.current_seq = target_seq;
        self.state = fold_projection(&self.store, &self.aggregate_id, target_seq)?;

        Ok(undone)
    }

    /// Undo all the way back to the beginning (seq 0).
    /// Returns the number of events undone.
    pub fn undo_all(&mut self) -> Result<u32> {
        let undone = self.current_seq as u32;
        if undone == 0 {
            return Ok(0);
        }
        self.current_seq = 0;
        self.state = fold_projection(&self.store, &self.aggregate_id, 0)?;
        Ok(undone)
    }

    /// Redo `count` events forward from the current position.
    /// Returns the number of events actually redone.
    ///
    /// Only events that are already in the log can be redone.
    /// New events must be created via `execute`.
    pub fn redo(&mut self, count: u32) -> Result<u32> {
        let total = self.store.event_count(&self.aggregate_id)?;

        if self.current_seq >= total {
            return Err(Error::NothingToRedo { seq: total });
        }

        let target_seq = std::cmp::min(total, self.current_seq + count as u64);
        let redone = (target_seq - self.current_seq) as u32;

        self.current_seq = target_seq;
        self.state = fold_projection(&self.store, &self.aggregate_id, target_seq)?;

        Ok(redone)
    }

    /// Redo all remaining events.
    /// Returns the number of events redone.
    pub fn redo_all(&mut self) -> Result<u32> {
        let total = self.store.event_count(&self.aggregate_id)?;
        if self.current_seq >= total {
            return Ok(0);
        }
        let redone = (total - self.current_seq) as u32;
        self.current_seq = total;
        self.state = fold_projection(&self.store, &self.aggregate_id, total)?;
        Ok(redone)
    }

    /// Get the full event history for this aggregate.
    pub fn history(&self) -> Result<Vec<Event>> {
        self.store.get_all(&self.aggregate_id)
    }

    /// Rebuild the state from the event log — a full replay.
    /// Returns the materialized state at the latest committed seq.
    /// This is used for INV-5 verification: replay must produce identical state.
    pub fn rebuild(&self) -> Result<HashMap<String, serde_json::Value>> {
        let total = self.store.event_count(&self.aggregate_id)?;
        let proj = fold_projection(&self.store, &self.aggregate_id, total)?;
        Ok(proj.as_map().clone())
    }

    /// Rebuild the state up to a specific sequence number.
    pub fn rebuild_up_to(&self, seq: u64) -> Result<HashMap<String, serde_json::Value>> {
        let proj = fold_projection(&self.store, &self.aggregate_id, seq)?;
        Ok(proj.as_map().clone())
    }
}

/// Fold events from seq 1 up to `max_seq` into a HashMapProjection.
///
/// Uses the Projection trait's deterministic fold: apply_event for each event
/// in sequence order. This is the canonical state reconstruction path (INV-5).
fn fold_projection(store: &EventStore, aggregate_id: &str, max_seq: u64) -> Result<HashMapProjection> {
    if max_seq == 0 {
        return Ok(HashMapProjection::new());
    }

    let events = store.get_up_to(aggregate_id, max_seq)?;
    Ok(HashMapProjection::rebuild(&events))
}

/// Get the current time in milliseconds since Unix epoch.
fn timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::EventStore;

    fn setup() -> Engine {
        let store = EventStore::open_in_memory().unwrap();
        Engine::new(store, "global").unwrap()
    }

    #[test]
    fn test_execute_set_and_get_state() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();

        assert_eq!(engine.state().get("hp").unwrap(), &serde_json::json!(100));
        assert_eq!(engine.current_seq(), 1);
        assert_eq!(engine.total_events().unwrap(), 1);
    }

    #[test]
    fn test_execute_delete() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();
        engine
            .execute(Command::Delete {
                key: "hp".into(),
            })
            .unwrap();

        assert!(engine.state().get("hp").is_none());
        assert_eq!(engine.current_seq(), 2);
        assert_eq!(engine.total_events().unwrap(), 2);
    }

    #[test]
    fn test_undo_single() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "mp".into(),
                value: serde_json::json!(50),
            })
            .unwrap();

        let undone = engine.undo(1).unwrap();
        assert_eq!(undone, 1);
        assert_eq!(engine.current_seq(), 1);
        // hp should still be there, mp should not.
        assert_eq!(engine.state().get("hp").unwrap(), &serde_json::json!(100));
        assert!(engine.state().get("mp").is_none());
    }

    #[test]
    fn test_undo_multiple() {
        let mut engine = setup();
        for i in 1..=5 {
            engine
                .execute(Command::Set {
                    key: format!("k{}", i),
                    value: serde_json::json!(i),
                })
                .unwrap();
        }

        let undone = engine.undo(3).unwrap();
        assert_eq!(undone, 3);
        assert_eq!(engine.current_seq(), 2);
        assert!(engine.state().get("k1").is_some());
        assert!(engine.state().get("k2").is_some());
        assert!(engine.state().get("k3").is_none());
    }

    #[test]
    fn test_undo_all() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();

        let undone = engine.undo_all().unwrap();
        assert_eq!(undone, 1);
        assert_eq!(engine.current_seq(), 0);
        assert!(engine.state().is_empty());
    }

    #[test]
    fn test_nothing_to_undo() {
        let mut engine = setup();
        let result = engine.undo(1);
        assert!(result.is_err());
        if let Err(Error::NothingToUndo) = result {
            // expected
        } else {
            panic!("Expected NothingToUndo");
        }
    }

    #[test]
    fn test_redo_after_undo() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "mp".into(),
                value: serde_json::json!(50),
            })
            .unwrap();

        engine.undo(1).unwrap();

        let redone = engine.redo(1).unwrap();
        assert_eq!(redone, 1);
        assert_eq!(engine.current_seq(), 2);
        assert_eq!(engine.state().get("mp").unwrap(), &serde_json::json!(50));
    }

    #[test]
    fn test_redo_all() {
        let mut engine = setup();
        for i in 1..=3 {
            engine
                .execute(Command::Set {
                    key: format!("k{}", i),
                    value: serde_json::json!(i),
                })
                .unwrap();
        }

        engine.undo(2).unwrap();
        assert_eq!(engine.current_seq(), 1);

        let redone = engine.redo_all().unwrap();
        assert_eq!(redone, 2);
        assert_eq!(engine.current_seq(), 3);
    }

    #[test]
    fn test_nothing_to_redo() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();

        let result = engine.redo(1);
        assert!(result.is_err());
    }

    #[test]
    fn test_rebuild_consistency_inv5() {
        // INV-5: replay (rebuild) must produce the same state as the current materialized state.
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "mp".into(),
                value: serde_json::json!(50),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "name".into(),
                value: serde_json::json!("Boss"),
            })
            .unwrap();

        let rebuilt = engine.rebuild().unwrap();
        assert_eq!(engine.state(), &rebuilt);
    }

    #[test]
    fn test_undo_redo_restart_replay_inv5() {
        // INV-5: undo → redo then verify rebuild matches state.
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "a".into(),
                value: serde_json::json!(1),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "b".into(),
                value: serde_json::json!(2),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "c".into(),
                value: serde_json::json!(3),
            })
            .unwrap();

        // Undo to seq 1, then redo to 3
        engine.undo(2).unwrap();
        engine.redo(2).unwrap();

        // State should match rebuild
        let rebuilt = engine.rebuild().unwrap();
        assert_eq!(engine.state(), &rebuilt);
    }

    #[test]
    fn test_history() {
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "hp".into(),
                value: serde_json::json!(100),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "mp".into(),
                value: serde_json::json!(50),
            })
            .unwrap();

        let history = engine.history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].seq, 1);
        assert_eq!(history[1].seq, 2);
    }

    #[test]
    fn test_new_command_after_undo_forks() {
        // When a new command is executed after an undo, it appends after the
        // current_seq (which is the total event count at that point).
        let mut engine = setup();
        engine
            .execute(Command::Set {
                key: "a".into(),
                value: serde_json::json!(1),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "b".into(),
                value: serde_json::json!(2),
            })
            .unwrap();
        engine
            .execute(Command::Set {
                key: "c".into(),
                value: serde_json::json!(3),
            })
            .unwrap();

        // Undo 1 event (back to seq 2)
        engine.undo(1).unwrap();
        assert_eq!(engine.current_seq(), 2);

        // New command — should update current_seq to total (3), then append as seq 4
        engine
            .execute(Command::Set {
                key: "d".into(),
                value: serde_json::json!(4),
            })
            .unwrap();

        assert_eq!(engine.current_seq(), 4);
        assert_eq!(engine.total_events().unwrap(), 4);
    }

    // ── Domain event tests (U2: Entities) ─────────────────────────

    #[test]
    fn test_create_entity_type() {
        let mut engine = setup();
        let event = engine
            .execute(Command::CreateEntityType {
                name: "Boss".into(),
            })
            .unwrap();
        assert_eq!(event.event_type, EventType::EntityTypeCreated);
        assert_eq!(engine.state().get("entity_type:Boss").unwrap(), &serde_json::json!({"fields": {}}));
    }

    #[test]
    fn test_create_entity_instance() {
        let mut engine = setup();
        engine
            .execute(Command::CreateEntityType {
                name: "Boss".into(),
            })
            .unwrap();
        let event = engine
            .execute(Command::CreateEntityInstance {
                entity_type: "Boss".into(),
                instance_id: "boss_1".into(),
            })
            .unwrap();
        assert_eq!(event.event_type, EventType::EntityInstanceCreated);
        let inst = engine.state().get("entity_instance:boss_1").unwrap();
        assert_eq!(inst["type"], "Boss");
    }

    #[test]
    fn test_set_entity_field() {
        let mut engine = setup();
        engine
            .execute(Command::CreateEntityType {
                name: "Boss".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEntityInstance {
                entity_type: "Boss".into(),
                instance_id: "boss_1".into(),
            })
            .unwrap();
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_1".into(),
                field: "hp".into(),
                value: serde_json::json!(500),
            })
            .unwrap();
        let inst = engine.state().get("entity_instance:boss_1").unwrap();
        assert_eq!(inst["fields"]["hp"], 500);
    }

    // ── Domain event tests (U3: Graph topology) ────────────────────

    #[test]
    fn test_create_node() {
        let mut engine = setup();
        let event = engine
            .execute(Command::CreateNode {
                node_id: "room_1".into(),
                label: "Central Hall".into(),
            })
            .unwrap();
        assert_eq!(event.event_type, EventType::NodeCreated);
        let node = engine.state().get("node:room_1").unwrap();
        assert_eq!(node["label"], "Central Hall");
        assert!(node["marks"].as_array().unwrap().is_empty());
        assert!(node["pois"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_remove_node() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "room_1".into(),
                label: "temp".into(),
            })
            .unwrap();
        engine
            .execute(Command::RemoveNode {
                node_id: "room_1".into(),
            })
            .unwrap();
        assert!(engine.state().get("node:room_1").is_none());
    }

    #[test]
    fn test_create_edge() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "a".into(),
                label: "A".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateNode {
                node_id: "b".into(),
                label: "B".into(),
            })
            .unwrap();
        let event = engine
            .execute(Command::CreateEdge {
                from_node: "a".into(),
                to_node: "b".into(),
                bidirectional: true,
            })
            .unwrap();
        assert_eq!(event.event_type, EventType::EdgeCreated);
        let edge = engine.state().get("edge:a->b").unwrap();
        assert!(edge["bidirectional"].as_bool().unwrap());
    }

    #[test]
    fn test_mark_node() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "room_1".into(),
                label: "Spawn Room".into(),
            })
            .unwrap();
        engine
            .execute(Command::MarkNode {
                node_id: "room_1".into(),
                mark: "spawn".into(),
            })
            .unwrap();
        let node = engine.state().get("node:room_1").unwrap();
        let marks = node["marks"].as_array().unwrap();
        assert_eq!(marks[0], "spawn");
    }

    // ── Domain event tests (U3: POI) ───────────────────────────────

    #[test]
    fn test_attach_poi() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "room_1".into(),
                label: "Boss Room".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEntityType {
                name: "Boss".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEntityInstance {
                entity_type: "Boss".into(),
                instance_id: "boss_1".into(),
            })
            .unwrap();
        engine
            .execute(Command::AttachPOI {
                node_id: "room_1".into(),
                poi_id: "poi_01".into(),
                entity_ref: Some("boss_1".into()),
            })
            .unwrap();
        let node = engine.state().get("node:room_1").unwrap();
        let pois = node["pois"].as_array().unwrap();
        assert_eq!(pois.len(), 1);
        assert_eq!(pois[0]["poi_id"], "poi_01");
        assert_eq!(pois[0]["entity_ref"], "boss_1");
    }

    #[test]
    fn test_detach_poi() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "room_1".into(),
                label: "Room".into(),
            })
            .unwrap();
        engine
            .execute(Command::AttachPOI {
                node_id: "room_1".into(),
                poi_id: "poi_01".into(),
                entity_ref: None,
            })
            .unwrap();
        engine
            .execute(Command::DetachPOI {
                node_id: "room_1".into(),
                poi_id: "poi_01".into(),
            })
            .unwrap();
        let node = engine.state().get("node:room_1").unwrap();
        let pois = node["pois"].as_array().unwrap();
        assert!(pois.is_empty());
    }

    // ── U3-specific completion criteria (SPEC §5.7) ─────────────

    #[test]
    fn test_u3_topology_5_rooms_central_branches_shortcut() {
        // SPEC U3 criterion 1: "5 rooms, central ↔ three branches bidirectional,
        // one unidirectional shortcut" expressed as typed data.
        let mut engine = setup();

        // Create 5 rooms: central hall + 3 branch rooms + 1 dead-end
        engine.execute(Command::CreateNode{node_id:"central".into(), label:"Central Hall".into()}).unwrap();
        engine.execute(Command::CreateNode{node_id:"branch_a".into(), label:"Armory".into()}).unwrap();
        engine.execute(Command::CreateNode{node_id:"branch_b".into(), label:"Library".into()}).unwrap();
        engine.execute(Command::CreateNode{node_id:"branch_c".into(), label:"Throne Room".into()}).unwrap();
        engine.execute(Command::CreateNode{node_id:"dead_end".into(), label:"Vault".into()}).unwrap();

        // Central ↔ three branches (bidirectional)
        engine.execute(Command::CreateEdge{from_node:"central".into(),to_node:"branch_a".into(),bidirectional:true}).unwrap();
        engine.execute(Command::CreateEdge{from_node:"central".into(),to_node:"branch_b".into(),bidirectional:true}).unwrap();
        engine.execute(Command::CreateEdge{from_node:"central".into(),to_node:"branch_c".into(),bidirectional:true}).unwrap();

        // One unidirectional shortcut: branch_c → dead_end (one-way)
        engine.execute(Command::CreateEdge{from_node:"branch_c".into(),to_node:"dead_end".into(),bidirectional:false}).unwrap();

        // Mark spawn point on central
        engine.execute(Command::MarkNode{node_id:"central".into(),mark:"spawn".into()}).unwrap();
        // Mark shortcut on the one-way edge's target
        engine.execute(Command::MarkNode{node_id:"dead_end".into(),mark:"shortcut".into()}).unwrap();

        // Verify all 5 nodes exist
        assert!(engine.state().get("node:central").is_some());
        assert!(engine.state().get("node:branch_a").is_some());
        assert!(engine.state().get("node:branch_b").is_some());
        assert!(engine.state().get("node:branch_c").is_some());
        assert!(engine.state().get("node:dead_end").is_some());

        // Verify bidirectional edges
        let e1 = engine.state().get("edge:central->branch_a").unwrap();
        assert!(e1["bidirectional"].as_bool().unwrap());
        let e2 = engine.state().get("edge:central->branch_b").unwrap();
        assert!(e2["bidirectional"].as_bool().unwrap());
        let e3 = engine.state().get("edge:central->branch_c").unwrap();
        assert!(e3["bidirectional"].as_bool().unwrap());

        // Verify unidirectional edge
        let e4 = engine.state().get("edge:branch_c->dead_end").unwrap();
        assert!(!e4["bidirectional"].as_bool().unwrap());

        // Verify marks
        let central = engine.state().get("node:central").unwrap();
        assert!(central["marks"].as_array().unwrap().contains(&serde_json::json!("spawn")));
        let vault = engine.state().get("node:dead_end").unwrap();
        assert!(vault["marks"].as_array().unwrap().contains(&serde_json::json!("shortcut")));

        // Verify everything went through event log
        let history = engine.history().unwrap();
        let node_count = history.iter().filter(|e| e.event_type == EventType::NodeCreated).count();
        assert_eq!(node_count, 5);
        let edge_count = history.iter().filter(|e| e.event_type == EventType::EdgeCreated).count();
        assert_eq!(edge_count, 4);
        let mark_count = history.iter().filter(|e| e.event_type == EventType::NodeMarked).count();
        assert_eq!(mark_count, 2);

        // Verify undo/redo: undo the shortcut mark, verify it's gone, redo it back
        let seq_before = engine.current_seq();
        engine.undo(1).unwrap();
        let vault_undone = engine.state().get("node:dead_end").unwrap();
        assert!(!vault_undone["marks"].as_array().unwrap().contains(&serde_json::json!("shortcut")));
        engine.redo(1).unwrap();
        let vault_redone = engine.state().get("node:dead_end").unwrap();
        assert!(vault_redone["marks"].as_array().unwrap().contains(&serde_json::json!("shortcut")));
        assert_eq!(engine.current_seq(), seq_before);
    }

    #[test]
    fn test_u3_poi_with_boss_entity_reference() {
        // SPEC U3 criterion 2: "某节点 POI 成功挂上 U2 的一个 Boss 实例"
        let mut engine = setup();

        // Create a node (room)
        engine.execute(Command::CreateNode{node_id:"boss_room".into(), label:"Dragon's Lair".into()}).unwrap();

        // U2: define Boss entity type
        engine.execute(Command::CreateEntityType{name:"Boss".into()}).unwrap();

        // U2: create Boss instance with fields
        engine.execute(Command::CreateEntityInstance{entity_type:"Boss".into(), instance_id:"dragon_01".into()}).unwrap();
        engine.execute(Command::SetEntityField{instance_id:"dragon_01".into(),field:"name".into(),value:serde_json::json!("Ancient Red Dragon")}).unwrap();
        engine.execute(Command::SetEntityField{instance_id:"dragon_01".into(),field:"hp".into(),value:serde_json::json!(5000)}).unwrap();
        engine.execute(Command::SetEntityField{instance_id:"dragon_01".into(),field:"difficulty".into(),value:serde_json::json!("nightmare")}).unwrap();

        // U3: attach POI referencing the Boss instance
        engine.execute(Command::AttachPOI{node_id:"boss_room".into(),poi_id:"poi_boss".into(),entity_ref:Some("dragon_01".into())}).unwrap();

        // Verify POI is on the node with correct entity reference
        let node = engine.state().get("node:boss_room").unwrap();
        let pois = node["pois"].as_array().unwrap();
        assert_eq!(pois.len(), 1);
        assert_eq!(pois[0]["poi_id"], "poi_boss");
        assert_eq!(pois[0]["entity_ref"], "dragon_01");

        // Verify the referenced entity exists and has its fields intact
        let boss = engine.state().get("entity_instance:dragon_01").unwrap();
        assert_eq!(boss["type"], "Boss");
        assert_eq!(boss["fields"]["name"], "Ancient Red Dragon");
        assert_eq!(boss["fields"]["hp"], 5000);

        // Verify all operations went through event log
        let history = engine.history().unwrap();
        let node_events: Vec<_> = history.iter().filter(|e| e.event_type == EventType::NodeCreated).collect();
        assert_eq!(node_events.len(), 1);
        let poi_events: Vec<_> = history.iter().filter(|e| e.event_type == EventType::POIAttached).collect();
        assert_eq!(poi_events.len(), 1);

        // Verify undo removes the POI, redo restores it
        engine.undo(1).unwrap();
        let node_undone = engine.state().get("node:boss_room").unwrap();
        assert!(node_undone["pois"].as_array().unwrap().is_empty());
        // But the entity instance still exists
        assert!(engine.state().get("entity_instance:dragon_01").is_some());

        engine.redo(1).unwrap();
        let node_redone = engine.state().get("node:boss_room").unwrap();
        assert_eq!(node_redone["pois"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_u3_unidirectional_edge_scenario() {
        // Explicit test for unidirectional edge: one-way from A→B, B cannot reach A.
        // This verifies edge directionality is stored and preserved.
        let mut engine = setup();

        engine.execute(Command::CreateNode{node_id:"a".into(), label:"Start".into()}).unwrap();
        engine.execute(Command::CreateNode{node_id:"b".into(), label:"End".into()}).unwrap();

        // Unidirectional: a → b
        engine.execute(Command::CreateEdge{from_node:"a".into(),to_node:"b".into(),bidirectional:false}).unwrap();

        let edge = engine.state().get("edge:a->b").unwrap();
        assert!(!edge["bidirectional"].as_bool().unwrap());

        // Verify the reverse edge does NOT exist (unidirectional means one-way)
        assert!(engine.state().get("edge:b->a").is_none());
    }

    // ── Invariant: domain events respect INV-2 & INV-5 ─────────────

    #[test]
    fn test_domain_events_in_event_log_inv2() {
        let mut engine = setup();
        let initial = engine.total_events().unwrap();

        engine
            .execute(Command::CreateNode {
                node_id: "a".into(),
                label: "A".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEntityType {
                name: "Item".into(),
            })
            .unwrap();

        assert_eq!(engine.total_events().unwrap(), initial + 2);
        let history = engine.history().unwrap();
        assert_eq!(history[0].event_type, EventType::NodeCreated);
        assert_eq!(history[1].event_type, EventType::EntityTypeCreated);
    }

    #[test]
    fn test_domain_events_replay_inv5() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "a".into(),
                label: "A".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateNode {
                node_id: "b".into(),
                label: "B".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEdge {
                from_node: "a".into(),
                to_node: "b".into(),
                bidirectional: false,
            })
            .unwrap();

        let rebuilt = engine.rebuild().unwrap();
        assert_eq!(engine.state(), &rebuilt,
            "INV-5: domain event replay must produce identical state"
        );
    }

    #[test]
    fn test_domain_events_undo_redo() {
        let mut engine = setup();
        engine
            .execute(Command::CreateNode {
                node_id: "a".into(),
                label: "A".into(),
            })
            .unwrap();
        engine
            .execute(Command::MarkNode {
                node_id: "a".into(),
                mark: "spawn".into(),
            })
            .unwrap();

        // Undo the mark
        engine.undo(1).unwrap();
        let node = engine.state().get("node:a").unwrap();
        assert!(node["marks"].as_array().unwrap().is_empty());

        // Redo the mark
        engine.redo(1).unwrap();
        let node = engine.state().get("node:a").unwrap();
        assert_eq!(node["marks"].as_array().unwrap()[0], "spawn");
    }

    #[test]
    fn test_u2_boss_entity_model_a_minimal() {
        let mut engine = setup();

        // Define Boss type (含 hp 等数值字段)
        engine
            .execute(Command::CreateEntityType {
                name: "Boss".into(),
            })
            .unwrap();

        // Create two instances
        engine
            .execute(Command::CreateEntityInstance {
                entity_type: "Boss".into(),
                instance_id: "boss_red".into(),
            })
            .unwrap();
        engine
            .execute(Command::CreateEntityInstance {
                entity_type: "Boss".into(),
                instance_id: "boss_blue".into(),
            })
            .unwrap();

        // Fill different values, demonstrating built-in field types:
        // numeric (hp), text (name), enum (difficulty as string), reference (minion_ref as id)
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_red".into(),
                field: "hp".into(),
                value: serde_json::json!(150),
            })
            .unwrap();
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_red".into(),
                field: "name".into(),
                value: serde_json::json!("Red Dragon"),
            })
            .unwrap();
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_red".into(),
                field: "difficulty".into(),
                value: serde_json::json!("hard"),
            })
            .unwrap();
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_red".into(),
                field: "minion_ref".into(),
                value: serde_json::json!("minion_001"),
            })
            .unwrap();
        engine
            .execute(Command::SetEntityField {
                instance_id: "boss_blue".into(),
                field: "hp".into(),
                value: serde_json::json!(300),
            })
            .unwrap();

        // Verify state has the values
        let red = engine.state().get("entity_instance:boss_red").unwrap();
        assert_eq!(red["fields"]["hp"], 150);
        assert_eq!(red["fields"]["name"], "Red Dragon");
        let blue = engine.state().get("entity_instance:boss_blue").unwrap();
        assert_eq!(blue["fields"]["hp"], 300);

        // Verify all changes went through the event log (INV-2)
        let history = engine.history().unwrap();
        let field_events: Vec<_> = history
            .iter()
            .filter(|e| e.event_type == EventType::EntityInstanceFieldSet)
            .collect();
        assert!(
            field_events.len() >= 5,
            "All field sets must appear in the append-only event log"
        );

        // Verify undoable (INV-5): undo past the hp field set, verify it's gone,
        // then redo to restore everything.
        // Total events: 8 (1 type + 2 instances + 5 field sets).
        // Undo 5 to go back to seq 3 (before any field sets on boss_red).
        let undone = engine.undo(5).unwrap();
        assert_eq!(undone, 5);
        assert_eq!(engine.current_seq(), 3);
        let red_after_undo = engine.state().get("entity_instance:boss_red").unwrap();
        // hp was set at seq 4; after undoing to seq 3 it must be absent
        assert!(
            red_after_undo
                .get("fields")
                .unwrap()
                .get("hp")
                .is_none(),
            "Undo must revert the field sets via re-fold from log"
        );

        engine.redo(5).unwrap();
        let red_after_redo = engine.state().get("entity_instance:boss_red").unwrap();
        assert_eq!(red_after_redo["fields"]["hp"], 150);
    }
}
