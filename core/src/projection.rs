use std::collections::HashMap;

use crate::event::{Event, EventType};

/// A Projection materializes state by folding a sequence of events.
///
/// This is the formal interface for INV-5: state = fold(events).
/// Every projection is deterministic — given the same events, it produces
/// the same state. Domain-specific projections (entity registry, graph
/// topology, POI index) implement this trait.
pub trait Projection: Default + Clone {
    /// Apply a single event to this projection, mutating it in place.
    fn apply_event(&mut self, event: &Event);

    /// Rebuild this projection from a slice of events.
    /// This is the canonical way to restore state from the event log (INV-5).
    fn rebuild(events: &[Event]) -> Self {
        let mut state = Self::default();
        for event in events {
            state.apply_event(event);
        }
        state
    }
}

/// The default key-value projection used by the Engine.
///
/// All domain events (U2 entities, U3 topology, etc.) are folded into
/// namespace-prefixed keys in this map. This serves as the primary
/// materialized state until typed domain projections replace it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HashMapProjection {
    inner: HashMap<String, serde_json::Value>,
}

impl HashMapProjection {
    /// Create an empty projection.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Get a reference to the inner map.
    pub fn as_map(&self) -> &HashMap<String, serde_json::Value> {
        &self.inner
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.inner.get(key)
    }

    /// Check if a key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Check if the projection is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Projection for HashMapProjection {
    /// Apply a single event to the materialized state.
    ///
    /// For Set/Delete, the event directly modifies the key-value state.
    /// For domain-specific events (U2 entities, U3 topology), the event payload
    /// is stored under namespace-prefixed keys so the state map serves as a
    /// unified projection surface.
    fn apply_event(&mut self, event: &Event) {
        match event.event_type {
            EventType::Set => {
                if let (Some(key), Some(value)) = (
                    event.payload.get("key").and_then(|v| v.as_str()),
                    event.payload.get("value"),
                ) {
                    self.inner.insert(key.to_string(), value.clone());
                }
            }
            EventType::Delete => {
                if let Some(key) = event.payload.get("key").and_then(|v| v.as_str()) {
                    self.inner.remove(key);
                }
            }
            // Domain events: store under namespace-prefixed keys.
            EventType::EntityTypeCreated => {
                if let Some(name) = event.payload.get("name").and_then(|v| v.as_str()) {
                    self.inner.insert(
                        format!("entity_type:{name}"),
                        serde_json::json!({"fields": {}}),
                    );
                }
            }
            EventType::EntityInstanceCreated => {
                if let (Some(et), Some(id)) = (
                    event.payload.get("entity_type").and_then(|v| v.as_str()),
                    event.payload.get("instance_id").and_then(|v| v.as_str()),
                ) {
                    self.inner.insert(
                        format!("entity_instance:{id}"),
                        serde_json::json!({"type": et, "fields": {}}),
                    );
                }
            }
            EventType::EntityInstanceFieldSet => {
                if let (Some(id), Some(field), Some(value)) = (
                    event.payload.get("instance_id").and_then(|v| v.as_str()),
                    event.payload.get("field").and_then(|v| v.as_str()),
                    event.payload.get("value"),
                ) {
                    let key = format!("entity_instance:{id}");
                    if let Some(inst) = self.inner.get_mut(&key) {
                        if let Some(fields) = inst.get_mut("fields") {
                            fields[field] = value.clone();
                        }
                    }
                }
            }
            EventType::NodeCreated => {
                if let (Some(nid), Some(label)) = (
                    event.payload.get("node_id").and_then(|v| v.as_str()),
                    event.payload.get("label").and_then(|v| v.as_str()),
                ) {
                    self.inner.insert(
                        format!("node:{nid}"),
                        serde_json::json!({"label": label, "marks": [], "pois": []}),
                    );
                }
            }
            EventType::NodeRemoved => {
                if let Some(nid) = event.payload.get("node_id").and_then(|v| v.as_str()) {
                    self.inner.remove(&format!("node:{nid}"));
                }
            }
            EventType::EdgeCreated => {
                if let (Some(from), Some(to)) = (
                    event.payload.get("from").and_then(|v| v.as_str()),
                    event.payload.get("to").and_then(|v| v.as_str()),
                ) {
                    let bidirectional = event
                        .payload
                        .get("bidirectional")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.inner.insert(
                        format!("edge:{from}->{to}"),
                        serde_json::json!({"bidirectional": bidirectional}),
                    );
                }
            }
            EventType::EdgeRemoved => {
                if let (Some(from), Some(to)) = (
                    event.payload.get("from").and_then(|v| v.as_str()),
                    event.payload.get("to").and_then(|v| v.as_str()),
                ) {
                    self.inner.remove(&format!("edge:{from}->{to}"));
                }
            }
            EventType::NodeMarked => {
                if let (Some(nid), Some(mark)) = (
                    event.payload.get("node_id").and_then(|v| v.as_str()),
                    event.payload.get("mark").and_then(|v| v.as_str()),
                ) {
                    let key = format!("node:{nid}");
                    if let Some(node) = self.inner.get_mut(&key) {
                        if let Some(marks) = node.get_mut("marks") {
                            if let Some(arr) = marks.as_array_mut() {
                                arr.push(serde_json::json!(mark));
                            }
                        }
                    }
                }
            }
            EventType::POIAttached => {
                if let (Some(nid), Some(pid)) = (
                    event.payload.get("node_id").and_then(|v| v.as_str()),
                    event.payload.get("poi_id").and_then(|v| v.as_str()),
                ) {
                    let entity_ref = event
                        .payload
                        .get("entity_ref")
                        .and_then(|v| v.as_str());
                    let key = format!("node:{nid}");
                    if let Some(node) = self.inner.get_mut(&key) {
                        if let Some(pois) = node.get_mut("pois") {
                            if let Some(arr) = pois.as_array_mut() {
                                let mut poi = serde_json::json!({"poi_id": pid});
                                if let Some(eref) = entity_ref {
                                    poi["entity_ref"] = serde_json::json!(eref);
                                }
                                arr.push(poi);
                            }
                        }
                    }
                }
            }
            EventType::POIDetached => {
                if let (Some(nid), Some(pid)) = (
                    event.payload.get("node_id").and_then(|v| v.as_str()),
                    event.payload.get("poi_id").and_then(|v| v.as_str()),
                ) {
                    let key = format!("node:{nid}");
                    if let Some(node) = self.inner.get_mut(&key) {
                        if let Some(pois) = node.get_mut("pois") {
                            if let Some(arr) = pois.as_array_mut() {
                                arr.retain(|p| {
                                    p.get("poi_id").and_then(|v| v.as_str())
                                        != Some(pid)
                                });
                            }
                        }
                    }
                }
            }
            // SchemaEvolved / Compensate: stored as raw entries.
            EventType::SchemaEvolved | EventType::Compensate => {
                self.inner.insert(
                    format!("event:{}", event.seq),
                    event.payload.clone(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;

    fn make_event(
        seq: u64,
        event_type: EventType,
        payload: serde_json::Value,
    ) -> Event {
        Event::new(seq, "global", event_type, payload, 1000)
    }

    #[test]
    fn test_hashmap_projection_set_and_get() {
        let mut proj = HashMapProjection::new();
        let event = make_event(1, EventType::Set, serde_json::json!({"key": "hp", "value": 100}));
        proj.apply_event(&event);
        assert_eq!(proj.get("hp").unwrap(), &serde_json::json!(100));
    }

    #[test]
    fn test_hashmap_projection_delete() {
        let mut proj = HashMapProjection::new();
        proj.apply_event(&make_event(1, EventType::Set, serde_json::json!({"key": "hp", "value": 100})));
        proj.apply_event(&make_event(2, EventType::Delete, serde_json::json!({"key": "hp"})));
        assert!(proj.get("hp").is_none());
    }

    #[test]
    fn test_hashmap_projection_rebuild() {
        let events = vec![
            make_event(1, EventType::Set, serde_json::json!({"key": "a", "value": 1})),
            make_event(2, EventType::Set, serde_json::json!({"key": "b", "value": 2})),
            make_event(3, EventType::Set, serde_json::json!({"key": "c", "value": 3})),
        ];
        let proj = HashMapProjection::rebuild(&events);
        assert_eq!(proj.get("a").unwrap(), &serde_json::json!(1));
        assert_eq!(proj.get("b").unwrap(), &serde_json::json!(2));
        assert_eq!(proj.get("c").unwrap(), &serde_json::json!(3));
        assert_eq!(proj.len(), 3);
    }

    #[test]
    fn test_hashmap_projection_rebuild_empty() {
        let proj = HashMapProjection::rebuild(&[]);
        assert!(proj.is_empty());
    }

    #[test]
    fn test_hashmap_projection_deterministic() {
        // INV-5: same events → identical projection
        let events = vec![
            make_event(1, EventType::Set, serde_json::json!({"key": "hp", "value": 100})),
            make_event(2, EventType::Set, serde_json::json!({"key": "mp", "value": 50})),
        ];
        let p1 = HashMapProjection::rebuild(&events);
        let p2 = HashMapProjection::rebuild(&events);
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_hashmap_projection_node_events() {
        let events = vec![
            make_event(1, EventType::NodeCreated, serde_json::json!({"node_id": "room_1", "label": "Central"})),
            make_event(2, EventType::NodeCreated, serde_json::json!({"node_id": "room_2", "label": "Branch"})),
            make_event(3, EventType::EdgeCreated, serde_json::json!({"from": "room_1", "to": "room_2", "bidirectional": true})),
            make_event(4, EventType::NodeMarked, serde_json::json!({"node_id": "room_1", "mark": "spawn"})),
        ];
        let proj = HashMapProjection::rebuild(&events);

        let node1 = proj.get("node:room_1").unwrap();
        assert_eq!(node1["label"], "Central");
        assert_eq!(node1["marks"].as_array().unwrap().len(), 1);

        let edge = proj.get("edge:room_1->room_2").unwrap();
        assert!(edge["bidirectional"].as_bool().unwrap());
    }

    #[test]
    fn test_hashmap_projection_entity_events() {
        let events = vec![
            make_event(1, EventType::EntityTypeCreated, serde_json::json!({"name": "Boss"})),
            make_event(2, EventType::EntityInstanceCreated, serde_json::json!({"entity_type": "Boss", "instance_id": "boss_1"})),
            make_event(3, EventType::EntityInstanceFieldSet, serde_json::json!({"instance_id": "boss_1", "field": "hp", "value": 500})),
        ];
        let proj = HashMapProjection::rebuild(&events);

        assert!(proj.contains_key("entity_type:Boss"));
        let inst = proj.get("entity_instance:boss_1").unwrap();
        assert_eq!(inst["type"], "Boss");
        assert_eq!(inst["fields"]["hp"], 500);
    }

    #[test]
    fn test_hashmap_projection_poi_events() {
        let events = vec![
            make_event(1, EventType::NodeCreated, serde_json::json!({"node_id": "room_1", "label": "Room"})),
            make_event(2, EventType::POIAttached, serde_json::json!({"node_id": "room_1", "poi_id": "poi_01", "entity_ref": "boss_1"})),
        ];
        let proj = HashMapProjection::rebuild(&events);

        let node = proj.get("node:room_1").unwrap();
        let pois = node["pois"].as_array().unwrap();
        assert_eq!(pois.len(), 1);
        assert_eq!(pois[0]["poi_id"], "poi_01");
        assert_eq!(pois[0]["entity_ref"], "boss_1");

        // Now detach
        let detach_event = make_event(3, EventType::POIDetached, serde_json::json!({"node_id": "room_1", "poi_id": "poi_01"}));
        let mut proj2 = proj.clone();
        proj2.apply_event(&detach_event);
        let node2 = proj2.get("node:room_1").unwrap();
        assert!(node2["pois"].as_array().unwrap().is_empty());
    }
}
