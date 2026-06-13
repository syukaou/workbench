use std::cell::RefCell;

use crate::error::Result;
use crate::event::Event;

/// In-memory append-only event store (WASM-compatible, no SQLite).
///
/// Same public API as `EventStore`, but stores events in a `Vec<Event>`.
/// Used when the `native` feature is disabled (e.g., wasm32 target).
/// Internal mutability via `RefCell` — safe for single-threaded WASM.
pub struct MemoryStore {
    events: RefCell<Vec<Event>>,
}

impl MemoryStore {
    /// Open a MemoryStore (path is ignored; always in-memory).
    pub fn open(_path: &str) -> Result<Self> {
        Self::open_in_memory()
    }

    /// Create an empty in-memory store.
    pub fn open_in_memory() -> Result<Self> {
        Ok(MemoryStore {
            events: RefCell::new(Vec::new()),
        })
    }

    /// Append a single event to the in-memory log.
    pub fn append(&self, event: &Event) -> Result<Event> {
        let mut events = self.events.borrow_mut();
        // Enforce sequence uniqueness (same as SQLite UNIQUE constraint).
        if events
            .iter()
            .any(|e| e.seq == event.seq && e.aggregate_id == event.aggregate_id)
        {
            return Err(crate::error::Error::Other(format!(
                "UNIQUE constraint failed: events.aggregate_id, events.seq ({} / {})",
                event.aggregate_id, event.seq
            )));
        }
        let id = (events.len() + 1) as u64;
        let persisted = Event {
            id,
            seq: event.seq,
            aggregate_id: event.aggregate_id.clone(),
            event_type: event.event_type.clone(),
            payload: event.payload.clone(),
            timestamp: event.timestamp,
        };
        events.push(persisted.clone());
        Ok(persisted)
    }

    /// Get all events for an aggregate, ordered by seq ASC.
    pub fn get_all(&self, aggregate_id: &str) -> Result<Vec<Event>> {
        let events = self.events.borrow();
        let mut result: Vec<Event> = events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .cloned()
            .collect();
        result.sort_by_key(|e| e.seq);
        Ok(result)
    }

    /// Get events up to (and including) `max_seq`, ordered by seq ASC.
    pub fn get_up_to(&self, aggregate_id: &str, max_seq: u64) -> Result<Vec<Event>> {
        let events = self.events.borrow();
        let mut result: Vec<Event> = events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id && e.seq <= max_seq)
            .cloned()
            .collect();
        result.sort_by_key(|e| e.seq);
        Ok(result)
    }

    /// Return the total number of events for an aggregate.
    pub fn event_count(&self, aggregate_id: &str) -> Result<u64> {
        let events = self.events.borrow();
        Ok(events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .count() as u64)
    }

    /// Get the next sequence number for an aggregate.
    pub fn next_seq(&self, aggregate_id: &str) -> Result<u64> {
        let events = self.events.borrow();
        let max_seq = events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .map(|e| e.seq)
            .max()
            .unwrap_or(0);
        Ok(max_seq + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;

    #[test]
    fn test_append_and_retrieve() {
        let store = MemoryStore::open_in_memory().unwrap();
        let event = Event::new(
            1,
            "global",
            EventType::Set,
            serde_json::json!({"key": "hp", "value": 100}),
            1000,
        );
        let persisted = store.append(&event).unwrap();
        assert_eq!(persisted.id, 1);
        assert_eq!(persisted.seq, 1);

        let events = store.get_all("global").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["key"], "hp");
    }

    #[test]
    fn test_seq_uniqueness_is_enforced() {
        let store = MemoryStore::open_in_memory().unwrap();
        let event = Event::new(
            1,
            "global",
            EventType::Set,
            serde_json::json!({"key": "hp"}),
            1000,
        );
        store.append(&event).unwrap();
        let result = store.append(&event);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_up_to() {
        let store = MemoryStore::open_in_memory().unwrap();
        for i in 1..=5 {
            let event = Event::new(
                i,
                "global",
                EventType::Set,
                serde_json::json!({"seq": i}),
                1000,
            );
            store.append(&event).unwrap();
        }
        let events = store.get_up_to("global", 3).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].seq, 3);
    }

    #[test]
    fn test_next_seq() {
        let store = MemoryStore::open_in_memory().unwrap();
        assert_eq!(store.next_seq("global").unwrap(), 1);
        let event = Event::new(1, "global", EventType::Set, serde_json::json!({}), 1000);
        store.append(&event).unwrap();
        assert_eq!(store.next_seq("global").unwrap(), 2);
    }
}
