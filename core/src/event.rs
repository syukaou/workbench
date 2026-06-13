use serde::{Deserialize, Serialize};

/// An append-only typed event in the event log.
///
/// Every mutation to the system state is recorded as exactly one Event.
/// Events are immutable once written — INV-5 demands append-only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// Unique event id (rowid in SQLite). 0 means not yet persisted.
    pub id: u64,
    /// Monotonic sequence number within an aggregate.
    pub seq: u64,
    /// Aggregate identifier (for multi-aggregate isolation; MVP uses "global").
    pub aggregate_id: String,
    /// The type of event — determines how it's applied during fold.
    pub event_type: EventType,
    /// Typed payload serialized as JSON.
    pub payload: serde_json::Value,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

/// The type of an event. Determines how the fold function applies it to state.
///
/// MVP supports Set and Delete. Compensating events (for undo-as-compensation)
/// use the same types — the payload carries the reversal semantics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Set a key to a value in the state map.
    Set,
    /// Remove a key from the state map.
    Delete,
}

impl Event {
    /// Create a new event (not yet persisted — id=0).
    pub fn new(
        seq: u64,
        aggregate_id: impl Into<String>,
        event_type: EventType,
        payload: serde_json::Value,
        timestamp: i64,
    ) -> Self {
        Event {
            id: 0,
            seq,
            aggregate_id: aggregate_id.into(),
            event_type,
            payload,
            timestamp,
        }
    }
}
