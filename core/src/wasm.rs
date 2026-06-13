//! WASM IPC bridge: exposes get_state() and execute_command() to JavaScript.
//!
//! Uses a thread-local in-memory WorkbenchCore instance (no SQLite, no filesystem).
//! The core is lazily initialized on first access.

use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use crate::contract::WorkbenchCore;
use crate::engine::Command;


// ── Global core instance ─────────────────────────────────────────────

thread_local! {
    static CORE: RefCell<Option<WorkbenchCore>> = RefCell::new(None);
}

/// Initialize the core if needed. Panics on init failure (WASM has no recovery).
fn ensure_core() {
    CORE.with(|cell| {
        if cell.borrow().is_none() {
            let core = WorkbenchCore::open_in_memory("global")
                .expect("Failed to initialize WorkbenchCore in WASM");
            *cell.borrow_mut() = Some(core);
        }
    });
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
                return JsValue::from_str(
                    &format!(r#"{{"ok":false,"error":"{}"}}"#, e),
                );
            }
        },
        Err(e) => {
            return JsValue::from_str(
                &format!(r#"{{"ok":false,"error":"Invalid JSON: {}"}}"#, e),
            );
        }
    };

    CORE.with(|cell| {
        let mut core = cell.borrow_mut();
        let c = core.as_mut().unwrap();
        match c.execute_command(command) {
            Ok(event) => JsValue::from_str(
                &format!(r#"{{"ok":true,"seq":{}}}"#, event.seq),
            ),
            Err(e) => JsValue::from_str(
                &format!(r#"{{"ok":false,"error":"{}"}}"#, e),
            ),
        }
    })
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
            Ok(Command::RemoveEdge {
                from_node,
                to_node,
            })
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
        _ => Err(format!("Unknown command variant: {}", variant_name)),
    }
}
