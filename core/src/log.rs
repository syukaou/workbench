use rusqlite::{params, Connection};

use crate::error::Result;
use crate::event::{Event, EventType};

/// Append-only event store backed by SQLite.
///
/// Writes are append-only (INSERT only, no UPDATE/DELETE on events rows).
/// Reads support sequential folding. INV-5: events are never modified or deleted.
pub struct EventStore {
    conn: Connection,
}

impl EventStore {
    /// Open or create the event store at the given path.
    /// Creates the events table and enables WAL mode for performance.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Enable WAL for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        // Create the append-only events table.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                seq         INTEGER NOT NULL,
                aggregate_id TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                payload     TEXT NOT NULL,
                timestamp   INTEGER NOT NULL,
                UNIQUE(aggregate_id, seq)
            );
            CREATE INDEX IF NOT EXISTS idx_events_aggregate_seq
                ON events(aggregate_id, seq);",
        )?;

        Ok(EventStore { conn })
    }

    /// Open an in-memory store (for testing).
    pub fn open_in_memory() -> Result<Self> {
        // In-memory connections share memory only within the same Connection.
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            "CREATE TABLE events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                seq         INTEGER NOT NULL,
                aggregate_id TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                payload     TEXT NOT NULL,
                timestamp   INTEGER NOT NULL,
                UNIQUE(aggregate_id, seq)
            );
            CREATE INDEX idx_events_aggregate_seq
                ON events(aggregate_id, seq);",
        )?;

        Ok(EventStore { conn })
    }

    /// Append a single event to the log. Returns the event with its assigned id.
    ///
    /// This is the sole write path — all mutations must go through here (INV-2).
    pub fn append(&self, event: &Event) -> Result<Event> {
        let payload_str = serde_json::to_string(&event.payload)?;
        let event_type_str = serde_json::to_string(&event.event_type)?;
        // Strip JSON quotes around the string value.
        let event_type_str = event_type_str.trim_matches('"');

        self.conn.execute(
            "INSERT INTO events (seq, aggregate_id, event_type, payload, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.seq,
                event.aggregate_id,
                event_type_str,
                payload_str,
                event.timestamp,
            ],
        )?;

        let id = self.conn.last_insert_rowid() as u64;

        Ok(Event {
            id,
            seq: event.seq,
            aggregate_id: event.aggregate_id.clone(),
            event_type: event.event_type.clone(),
            payload: event.payload.clone(),
            timestamp: event.timestamp,
        })
    }

    /// Get all events for an aggregate, ordered by seq ascending.
    pub fn get_all(&self, aggregate_id: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, aggregate_id, event_type, payload, timestamp
             FROM events
             WHERE aggregate_id = ?1
             ORDER BY seq ASC",
        )?;

        let rows = stmt.query_map(params![aggregate_id], |row| {
            let event_type_str: String = row.get(3)?;
            let event_type: EventType = serde_json::from_str(&format!("\"{}\"", event_type_str))
                .unwrap_or(EventType::Set);
            let payload_str: String = row.get(4)?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

            Ok(Event {
                id: row.get(0)?,
                seq: row.get(1)?,
                aggregate_id: row.get(2)?,
                event_type,
                payload,
                timestamp: row.get(5)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Get events up to (and including) the given seq, ordered by seq ascending.
    pub fn get_up_to(&self, aggregate_id: &str, max_seq: u64) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, aggregate_id, event_type, payload, timestamp
             FROM events
             WHERE aggregate_id = ?1 AND seq <= ?2
             ORDER BY seq ASC",
        )?;

        let rows = stmt.query_map(params![aggregate_id, max_seq], |row| {
            let event_type_str: String = row.get(3)?;
            let event_type: EventType = serde_json::from_str(&format!("\"{}\"", event_type_str))
                .unwrap_or(EventType::Set);
            let payload_str: String = row.get(4)?;
            let payload: serde_json::Value =
                serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

            Ok(Event {
                id: row.get(0)?,
                seq: row.get(1)?,
                aggregate_id: row.get(2)?,
                event_type,
                payload,
                timestamp: row.get(5)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Get the next sequence number for an aggregate.
    pub fn next_seq(&self, aggregate_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM events WHERE aggregate_id = ?1",
            params![aggregate_id],
            |row| row.get(0),
        )?;
        Ok((count + 1) as u64)
    }

    /// Return the total number of events for an aggregate.
    pub fn event_count(&self, aggregate_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE aggregate_id = ?1",
            params![aggregate_id],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_retrieve() {
        let store = EventStore::open_in_memory().unwrap();
        let event = Event::new(1, "global", EventType::Set, serde_json::json!({"key": "hp", "value": 100}), 1000);
        let persisted = store.append(&event).unwrap();
        assert_eq!(persisted.id, 1);
        assert_eq!(persisted.seq, 1);

        let events = store.get_all("global").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["key"], "hp");
    }

    #[test]
    fn test_seq_uniqueness_is_enforced() {
        let store = EventStore::open_in_memory().unwrap();
        let event = Event::new(1, "global", EventType::Set, serde_json::json!({"key": "hp"}), 1000);
        store.append(&event).unwrap();
        // Duplicate seq should fail.
        let result = store.append(&event);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_up_to() {
        let store = EventStore::open_in_memory().unwrap();
        for i in 1..=5 {
            let event = Event::new(i, "global", EventType::Set, serde_json::json!({"seq": i}), 1000);
            store.append(&event).unwrap();
        }
        let events = store.get_up_to("global", 3).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].seq, 3);
    }

    #[test]
    fn test_next_seq() {
        let store = EventStore::open_in_memory().unwrap();
        assert_eq!(store.next_seq("global").unwrap(), 1);
        let event = Event::new(1, "global", EventType::Set, serde_json::json!({}), 1000);
        store.append(&event).unwrap();
        assert_eq!(store.next_seq("global").unwrap(), 2);
    }
}
