//! WASM IPC bridge: exposes get_state(), execute_command(), and propose() to JavaScript.
//!
//! Uses a thread-local in-memory WorkbenchCore instance (no SQLite, no filesystem).
//! The core is lazily initialized on first access and seeded with a default
//! 5-room topology on first creation.

use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use crate::contract::WorkbenchCore;
use crate::engine::Command;
use crate::event::Event;

// ── Global core instance ─────────────────────────────────────────────

thread_local! {
    static CORE: RefCell<Option<WorkbenchCore>> = RefCell::new(None);
}

/// Initialize the core if needed, seeding default topology on first creation.
/// Panics on init failure (WASM has no recovery).
fn ensure_core() {
    CORE.with(|cell| {
        if cell.borrow().is_none() {
            let mut core = WorkbenchCore::open_in_memory("global")
                .expect("Failed to initialize WorkbenchCore in WASM");
            seed_topology(&mut core);
            *cell.borrow_mut() = Some(core);
        }
    });
}

/// Inject the default 5-room topology into a freshly-created core.
///
/// Topology:
///   Entrance Hall ↔ Armory (bidirectional)
///   Entrance Hall ↔ Library (bidirectional)
///   Garden → Vault (one-way shortcut)
fn seed_topology(core: &mut WorkbenchCore) {
    // ── U2: Entity types ─────────────────────────────────────────────
    core.create_entity_type("Boss")
        .expect("seed: create entity type Boss");
    core.create_entity_type("Item")
        .expect("seed: create entity type Item");
    core.create_entity_type("NPC")
        .expect("seed: create entity type NPC");

    // ── U3: Topology nodes ───────────────────────────────────────────
    // Create 5 rooms
    core.create_node("entrance", "Entrance Hall")
        .expect("seed: create entrance");
    core.create_node("armory", "Armory")
        .expect("seed: create armory");
    core.create_node("library", "Library")
        .expect("seed: create library");
    core.create_node("garden", "Garden")
        .expect("seed: create garden");
    core.create_node("vault", "Vault")
        .expect("seed: create vault");

    // Bidirectional: entrance ↔ armory, entrance ↔ library
    core.create_edge("entrance", "armory", true)
        .expect("seed: edge entrance-armory");
    core.create_edge("entrance", "library", true)
        .expect("seed: edge entrance-library");

    // One-way shortcut: garden → vault
    core.create_edge("garden", "vault", false)
        .expect("seed: edge garden-vault");

    // Mark entrance as spawn, vault as shortcut
    core.mark_node("entrance", "spawn")
        .expect("seed: mark entrance");
    core.mark_node("vault", "shortcut")
        .expect("seed: mark vault");
}

// ── Public WASM exports ──────────────────────────────────────────────

/// Get the current materialized state as a JSON string.
///
/// Returns a JSON object mapping namespace-prefixed keys to values.
/// The state is derived by folding all events in the in-memory log.
#[wasm_bindgen]
pub fn get_state() -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let core = cell.borrow();
        let state = core.as_ref().unwrap().get_state();
        let json = serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_string());
        JsValue::from_str(&json)
    })
}

/// Execute a command from a JSON string.
///
/// The JSON must be an externally-tagged Command:
/// ```json
/// {"CreateNode": {"node_id": "hall", "label": "Entrance Hall"}}
/// ```
///
/// Returns a JSON object: `{"ok": true, "seq": N}` on success,
/// or `{"ok": false, "error": "message"}` on failure.
#[wasm_bindgen]
pub fn execute_command(json_str: &str) -> JsValue {
    ensure_core();

    let command: Command = match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(val) => match parse_command(&val) {
            Ok(cmd) => cmd,
            Err(e) => {
                return JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e));
            }
        },
        Err(e) => {
            return JsValue::from_str(&format!(r#"{{"ok":false,"error":"Invalid JSON: {}"}}"#, e));
        }
    };

    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.execute_command(command) {
            Ok(event) => JsValue::from_str(&format!(r#"{{"ok":true,"seq":{}}}"#, event.seq)),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Export a full project snapshot as JSON (events + state).
///
/// Returns a JSON object:
/// ```json
/// {"version": 1, "events": [...], "state": {...}}
/// ```
/// The frontend can save this as a `.workbench.json` file.
#[wasm_bindgen]
pub fn export_snapshot() -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let core = cell.borrow();
        let c = core.as_ref().unwrap();
        match c.export_snapshot() {
            Ok(snapshot) => JsValue::from_str(&snapshot.to_string()),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Import a project snapshot, replacing the current state.
///
/// The JSON must contain an `events` array of serialized events.
/// Returns `{"ok": true}` on success, or `{"ok": false, "error": "..."}` on failure.
#[wasm_bindgen]
pub fn import_snapshot(json_str: &str) -> JsValue {
    ensure_core();

    // Parse the top-level JSON
    let snapshot: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            return JsValue::from_str(&format!(
                r#"{{"ok":false,"error":"Invalid JSON: {}"}}"#,
                e
            ));
        }
    };

    // Extract events array
    let events_arr = match snapshot.get("events").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return JsValue::from_str(r#"{"ok":false,"error":"Missing 'events' array in snapshot"}"#);
        }
    };

    // Deserialize each event
    let events: Vec<Event> = match events_arr
        .iter()
        .map(|v| serde_json::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(evts) => evts,
        Err(e) => {
            return JsValue::from_str(&format!(
                r#"{{"ok":false,"error":"Invalid event in snapshot: {}"}}"#,
                e
            ));
        }
    };

    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.import_snapshot(&events) {
            Ok(()) => JsValue::from_str(r#"{"ok":true}"#),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

// ── Undo / Redo (M1: expose the event-log undo to JS — INV-5) ─────────
// The UI must delegate undo/redo to the core's event log, never reimplement
// it. These wrap WorkbenchCore::{undo,redo,undo_all,redo_all} and report the
// cursor via undo_redo_status. NothingToUndo/Redo is a no-op (count 0), not
// an error, so a JS-side undo at seq 0 doesn't throw.

/// Undo `count` events via the event log. Returns `{"ok":true,"undone":N}`.
#[wasm_bindgen]
pub fn undo(count: u32) -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.undo(count) {
            Ok(n) => JsValue::from_str(&format!(r#"{{"ok":true,"undone":{}}}"#, n)),
            Err(crate::Error::NothingToUndo) => JsValue::from_str(r#"{"ok":true,"undone":0}"#),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Redo `count` events. Returns `{"ok":true,"redone":N}`.
#[wasm_bindgen]
pub fn redo(count: u32) -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.redo(count) {
            Ok(n) => JsValue::from_str(&format!(r#"{{"ok":true,"redone":{}}}"#, n)),
            Err(crate::Error::NothingToRedo { .. }) => {
                JsValue::from_str(r#"{"ok":true,"redone":0}"#)
            }
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Undo all events back to seq 0. Returns `{"ok":true,"undone":N}`.
#[wasm_bindgen]
pub fn undo_all() -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.undo_all() {
            Ok(n) => JsValue::from_str(&format!(r#"{{"ok":true,"undone":{}}}"#, n)),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Redo all remaining events. Returns `{"ok":true,"redone":N}`.
#[wasm_bindgen]
pub fn redo_all() -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.redo_all() {
            Ok(n) => JsValue::from_str(&format!(r#"{{"ok":true,"redone":{}}}"#, n)),
            Err(e) => JsValue::from_str(&format!(r#"{{"ok":false,"error":"{}"}}"#, e)),
        }
    })
}

/// Undo/redo cursor status: `{"current_seq":S,"total_events":T}`.
/// The UI derives canUndo = current_seq > 0, canRedo = current_seq < total_events.
#[wasm_bindgen]
pub fn undo_redo_status() -> JsValue {
    ensure_core();
    CORE.with(|cell| {
        let core = cell.borrow();
        let c = core.as_ref().unwrap();
        let seq = c.get_current_seq();
        let total = c.get_total_events().unwrap_or(0);
        JsValue::from_str(&format!(
            r#"{{"current_seq":{},"total_events":{}}}"#,
            seq, total
        ))
    })
}

/// Generate topology proposals from a natural language intent.
///
/// In WASM, we use a mock proposal generator (CLI is native-only, INV-4).
/// Returns a JSON array of command objects ready for `execute_command`.
///
/// The mock generator parses keywords from the intent:
/// - "branch" / "fork" → creates hub with branch nodes
/// - "loop" / "circle" → creates a cycle
/// - "shortcut" → creates a one-way shortcut edge
/// - "secret" → creates a hidden room with one-way entrance
/// - Otherwise → creates a simple linear chain
#[wasm_bindgen]
pub fn propose(intent: &str) -> JsValue {
    ensure_core();
    let commands = mock_propose(intent);
    let json = serde_json::to_string(&commands).unwrap_or_else(|_| "[]".to_string());
    JsValue::from_str(&json)
}

/// Build mock proposal commands from keyword-matched intent.
fn mock_propose(intent: &str) -> Vec<serde_json::Value> {
    let lower = intent.to_lowercase();
    let mut cmds: Vec<serde_json::Value> = Vec::new();

    if lower.contains("branch") || lower.contains("fork") || lower.contains("hub") {
        // Hub with two branches
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "hub", "label": "Central Hub"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "branch_a", "label": "Branch A"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "branch_b", "label": "Branch B"}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "hub", "to_node": "branch_a", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "hub", "to_node": "branch_b", "bidirectional": true}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "hub", "mark": "spawn"}}));
    } else if lower.contains("loop") || lower.contains("circle") || lower.contains("cycle") {
        // Circular loop: a → b → c → a
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "loop_a", "label": "Loop A"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "loop_b", "label": "Loop B"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "loop_c", "label": "Loop C"}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "loop_a", "to_node": "loop_b", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "loop_b", "to_node": "loop_c", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "loop_c", "to_node": "loop_a", "bidirectional": true}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "loop_a", "mark": "spawn"}}));
    } else if lower.contains("shortcut") || lower.contains("skip") {
        // Linear path with a shortcut
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "start", "label": "Start"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "mid", "label": "Midway"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "end", "label": "End"}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "start", "to_node": "mid", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "mid", "to_node": "end", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "start", "to_node": "end", "bidirectional": false}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "start", "mark": "spawn"}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "end", "mark": "shortcut"}}));
    } else if lower.contains("secret") || lower.contains("hidden") {
        // Hidden room accessible via one-way
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "main", "label": "Main Hall"}}));
        cmds.push(
            serde_json::json!({"CreateNode": {"node_id": "secret_room", "label": "Secret Room"}}),
        );
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "main", "to_node": "secret_room", "bidirectional": false}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "main", "mark": "spawn"}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "secret_room", "mark": "treasure"}}));
    } else {
        // Default: simple linear chain of 3 rooms
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "room_1", "label": "Room 1"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "room_2", "label": "Room 2"}}));
        cmds.push(serde_json::json!({"CreateNode": {"node_id": "room_3", "label": "Room 3"}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "room_1", "to_node": "room_2", "bidirectional": true}}));
        cmds.push(serde_json::json!({"CreateEdge": {"from_node": "room_2", "to_node": "room_3", "bidirectional": true}}));
        cmds.push(serde_json::json!({"MarkNode": {"node_id": "room_1", "mark": "spawn"}}));
    }

    cmds
}

// ── Command JSON parser (externally-tagged) ──────────────────────────

/// Parse a JSON value into a `Command` using externally-tagged format.
fn parse_command(val: &serde_json::Value) -> Result<Command, String> {
    let obj = val
        .as_object()
        .ok_or_else(|| "Command must be a JSON object".to_string())?;

    if obj.len() != 1 {
        return Err(format!(
            "Expected exactly one key (the command variant), got {}",
            obj.len()
        ));
    }

    let (variant_name, params_value) = obj.iter().next().unwrap();
    let params = params_value
        .as_object()
        .ok_or_else(|| "Command params must be a JSON object".to_string())?;

    let get_str = |key: &str| -> Result<String, String> {
        params
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Missing or non-string field: {}", key))
    };

    match variant_name.as_str() {
        "CreateNode" => {
            let node_id = get_str("node_id")?;
            let label = get_str("label")?;
            Ok(Command::CreateNode { node_id, label })
        }
        "RemoveNode" => {
            let node_id = get_str("node_id")?;
            Ok(Command::RemoveNode { node_id })
        }
        "CreateEdge" => {
            let from_node = get_str("from_node")?;
            let to_node = get_str("to_node")?;
            let bidirectional = params
                .get("bidirectional")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            Ok(Command::CreateEdge {
                from_node,
                to_node,
                bidirectional,
            })
        }
        "RemoveEdge" => {
            let from_node = get_str("from_node")?;
            let to_node = get_str("to_node")?;
            Ok(Command::RemoveEdge { from_node, to_node })
        }
        "MarkNode" => {
            let node_id = get_str("node_id")?;
            let mark = get_str("mark")?;
            Ok(Command::MarkNode { node_id, mark })
        }
        "AttachPOI" => {
            let node_id = get_str("node_id")?;
            let poi_id = get_str("poi_id")?;
            let entity_ref = params
                .get("entity_ref")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && *s != "null")
                .map(|s| s.to_string());
            Ok(Command::AttachPOI {
                node_id,
                poi_id,
                entity_ref,
            })
        }
        "DetachPOI" => {
            let node_id = get_str("node_id")?;
            let poi_id = get_str("poi_id")?;
            Ok(Command::DetachPOI { node_id, poi_id })
        }
        "CreateEntityType" => {
            let name = get_str("name")?;
            Ok(Command::CreateEntityType { name })
        }
        "CreateEntityInstance" => {
            let entity_type = get_str("entity_type")?;
            let instance_id = get_str("instance_id")?;
            Ok(Command::CreateEntityInstance {
                entity_type,
                instance_id,
            })
        }
        "SetEntityField" => {
            let instance_id = get_str("instance_id")?;
            let field = get_str("field")?;
            let value = params
                .get("value")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Ok(Command::SetEntityField {
                instance_id,
                field,
                value,
            })
        }
        _ => Err(format!("Unknown command variant: {}", variant_name)),
    }
}
