use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::event::{Event, EventType};
use crate::log::EventStore;

/// A command to mutate the system state. Every command goes through
/// validate → serialize → append to event log → update state (INV-2).
#[derive(Debug, Clone)]
pub enum Command {
    /// Set a key to a value.
    Set {
        key: String,
        value: serde_json::Value,
    },
    /// Delete a key.
    Delete {
        key: String,
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
    /// The materialized state at `current_seq`.
    state: HashMap<String, serde_json::Value>,
}

impl Engine {
    /// Create a new engine backed by the given EventStore.
    /// The state is initialized by folding all existing events.
    pub fn new(store: EventStore, aggregate_id: impl Into<String>) -> Result<Self> {
        let aggregate_id = aggregate_id.into();
        let current_seq = store.event_count(&aggregate_id)?;
        let state = fold_events(&store, &aggregate_id, current_seq)?;

        Ok(Engine {
            store,
            aggregate_id,
            current_seq,
            state,
        })
    }

    /// Get a snapshot of the current materialized state.
    pub fn state(&self) -> &HashMap<String, serde_json::Value> {
        &self.state
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
        };

        // Validate: check that the payload is valid JSON.
        serde_json::to_string(&payload).map_err(|e| Error::InvalidCommand(format!("Cannot serialize payload: {}", e)))?;

        let next_seq = self.current_seq + 1;
        let timestamp = timestamp_ms();

        let event = Event::new(next_seq, &self.aggregate_id, event_type, payload, timestamp);
        let persisted = self.store.append(&event)?;

        // Apply the event to the current state to keep it materialized.
        apply_event(&mut self.state, &persisted);
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
        self.state = fold_events(&self.store, &self.aggregate_id, target_seq)?;

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
        self.state = fold_events(&self.store, &self.aggregate_id, 0)?;
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
        self.state = fold_events(&self.store, &self.aggregate_id, target_seq)?;

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
        self.state = fold_events(&self.store, &self.aggregate_id, total)?;
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
        fold_events(&self.store, &self.aggregate_id, total)
    }

    /// Rebuild the state up to a specific sequence number.
    pub fn rebuild_up_to(&self, seq: u64) -> Result<HashMap<String, serde_json::Value>> {
        fold_events(&self.store, &self.aggregate_id, seq)
    }
}

/// Fold events from seq 1 up to `max_seq` into a HashMap state.
fn fold_events(store: &EventStore, aggregate_id: &str, max_seq: u64) -> Result<HashMap<String, serde_json::Value>> {
    let mut state = HashMap::new();
    if max_seq == 0 {
        return Ok(state);
    }

    let events = store.get_up_to(aggregate_id, max_seq)?;
    for event in &events {
        apply_event(&mut state, event);
    }

    Ok(state)
}

/// Apply a single event to the materialized state.
fn apply_event(state: &mut HashMap<String, serde_json::Value>, event: &Event) {
    match event.event_type {
        EventType::Set => {
            if let (Some(key), Some(value)) = (
                event.payload.get("key").and_then(|v| v.as_str()),
                event.payload.get("value"),
            ) {
                state.insert(key.to_string(), value.clone());
            }
        }
        EventType::Delete => {
            if let Some(key) = event.payload.get("key").and_then(|v| v.as_str()) {
                state.remove(key);
            }
        }
    }
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
}
