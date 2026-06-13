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

/// The type of a domain event. Determines how the fold function applies it to state.
///
/// Events are grouped by the MVP unit that introduces them:
/// - U1 (foundation): Set, Delete, SchemaEvolved, Compensate
/// - U2 (entities): EntityTypeCreated, EntityInstanceCreated, EntityInstanceFieldSet
/// - U3 (topology): NodeCreated, NodeRemoved, EdgeCreated, EdgeRemoved,
///                  NodeMarked, POIAttached, POIDetached
///
/// Compensating events (undo-as-compensation, INV-5) use Compensate — the
/// payload carries the event to be reversed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // ── U1: Generic / foundation ──
    /// Set a key to a value in the state map.
    Set,
    /// Remove a key from the state map.
    Delete,
    /// Schema evolution: add a field to a type, add a new entity type, etc.
    SchemaEvolved,
    /// Compensation event: reverses a previous event (INV-5 undo semantics).
    Compensate,

    // ── U2: Entity types and instances ──
    /// A new entity type was defined (e.g. "Boss", "Item").
    EntityTypeCreated,
    /// An instance of an entity type was created.
    EntityInstanceCreated,
    /// A field on an entity instance was set to a value.
    EntityInstanceFieldSet,

    // ── U3: Graph topology ──
    /// A node (room/area) was added to the graph.
    NodeCreated,
    /// A node was removed from the graph.
    NodeRemoved,
    /// An edge (connection) was added between two nodes.
    EdgeCreated,
    /// An edge was removed.
    EdgeRemoved,
    /// A semantic mark was applied to a node (spawn, shortcut, etc.).
    NodeMarked,

    // ── U3: POI ──
    /// A POI was attached to a node, optionally referencing an entity instance.
    POIAttached,
    /// A POI was detached from a node.
    POIDetached,
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
