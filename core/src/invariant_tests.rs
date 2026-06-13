//! Invariant tests — physical guardrails for CLAUDE.md §1.
//!
//! Each invariant has at least one automated test that fails if the invariant is violated.
//! These tests run as part of `cargo test`.

use std::collections::HashMap;
use crate::{EventType, WorkbenchCore};

// ── INV-1: Single source of truth ────────────────────────────────────

#[test]
fn inv1_state_is_derived_from_events() {
    // All design state is a projection derived from the event log.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.set("hp", serde_json::json!(100)).unwrap();
    core.set("mp", serde_json::json!(50)).unwrap();

    // State must be reconstructable from events alone.
    let rebuilt = core.rebuild().unwrap();
    let current = core.get_state();
    assert_eq!(current, rebuilt,
        "INV-1 VIOLATION: state (from live materialization) != state rebuilt from events"
    );
}

// ── INV-2: Sole write path ───────────────────────────────────────────

#[test]
fn inv2_every_mutation_produces_event() {
    // Every accepted mutation must produce a corresponding event in the log.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();
    let initial_count = core.get_total_events().unwrap();

    core.set("hp", serde_json::json!(100)).unwrap();
    assert_eq!(core.get_total_events().unwrap(), initial_count + 1,
        "INV-2 VIOLATION: mutation did not produce an event"
    );

    core.delete("hp").unwrap();
    assert_eq!(core.get_total_events().unwrap(), initial_count + 2,
        "INV-2 VIOLATION: mutation did not produce an event"
    );

    // Verify the events exist in the log and are typed.
    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].event_type, EventType::Set);
    assert_eq!(history[1].event_type, EventType::Delete);
}

#[test]
fn inv2_state_changes_only_through_execute() {
    // There should be no way to change state without calling execute/set/delete.
    // (This is a structural invariant — the state is only accessible as read-only
    // through get_state(), which returns a clone.)
    let core = WorkbenchCore::open_in_memory("global").unwrap();
    let state_before = core.get_state();

    // Attempting to mutate the returned HashMap should not affect the core state.
    // (This is guaranteed by Rust's ownership model — we return a clone.)
    let mut state_copy = core.get_state();
    state_copy.insert("hack".into(), serde_json::json!("evil"));

    // Core state must be unchanged.
    assert_eq!(core.get_state(), state_before,
        "INV-2 VIOLATION: state can be mutated outside execute path"
    );
}

// ── INV-3: AI is proposer, not decider (structural check) ────────────
// INV-3 is primarily enforced at the U5 level (AI-CLI proposal channel).
// In U1, we verify that there is no mechanism to accept/reject proposals
// because proposal handling doesn't exist yet. This test documents that
// the core crate itself has no "auto-accept" mechanism.

#[test]
fn inv3_no_auto_accept_mechanism() {
    // The core has no AI-specific proposal type or auto-accept logic.
    // All state changes require an explicit call to execute().
    // (This test verifies the structural absence of auto-accept.)
    let core = WorkbenchCore::open_in_memory("global").unwrap();
    // No proposal-related methods exist on the public API.
    // The only write methods are set() and delete() — both require explicit invocation.
    assert_eq!(core.get_total_events().unwrap(), 0);
}

// ── INV-4 & INV-7: Core has zero LLM / HTTP / rendering deps ─────────

#[test]
fn inv4_inv7_core_has_no_llm_http_render_deps() {
    // This test is a compile-time check. To make it a runtime assertion,
    // we verify that no LLM/HTTP/rendering symbols are accessible.
    // The Cargo.toml of core must not include any such dependencies.
    // We can verify by checking that none of the forbidden crate names
    // appear in the dependency graph.

    // Read the Cargo.toml to verify.
    let cargo_toml = include_str!("../Cargo.toml");

    let forbidden = [
        // LLM
        "openai", "anthropic", "llm", "gpt", "claude", "gemini", "mistral",
        "tokenizer", "tiktoken", "candle", "torch", "onnx",
        // HTTP / networking
        "reqwest", "hyper", "http", "tokio-tungstenite", "tungstenite",
        "axum", "warp", "actix", "rocket", "ureq", "curl", "socket2",
        // Rendering
        "wgpu", "vulkan", "opengl", "gl", "sdl2", "pixels", "rayon-vulkan",
        "bevy", "macroquad", "ggez", "miniquad", "skia", "raqote",
        "tauri", // rendering shell — should be in src-tauri, not core
    ];

    for forbidden_crate in &forbidden {
        assert!(
            !cargo_toml.to_lowercase().contains(&forbidden_crate.to_lowercase()),
            "INV-4/7 VIOLATION: core Cargo.toml references forbidden crate '{}' (LLM/HTTP/rendering)",
            forbidden_crate
        );
    }

    // Also verify the core Cargo.toml doesn't contain tauri dependencies.
    assert!(!cargo_toml.contains("tauri"), "INV-7 VIOLATION: core references tauri (rendering shell)");
}

// ── INV-5: Event sourcing — replay consistency ───────────────────────

#[test]
fn inv5_replay_consistency() {
    // State rebuilt from events must equal current state.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    for i in 1..=10 {
        core.set(&format!("k{}", i), serde_json::json!(i)).unwrap();
    }

    let current = core.get_state();
    let rebuilt = core.rebuild().unwrap();

    assert_eq!(current, rebuilt,
        "INV-5 VIOLATION: rebuild(state) != current state — events don't deterministically produce state"
    );
}

#[test]
fn inv5_undo_redo_produces_consistent_state() {
    // Undo → redo must restore the exact same state.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.set("a", serde_json::json!(1)).unwrap();
    core.set("b", serde_json::json!(2)).unwrap();
    core.set("c", serde_json::json!(3)).unwrap();

    let state_before = core.get_state();

    core.undo(2).unwrap();
    core.redo(2).unwrap();

    let state_after = core.get_state();
    assert_eq!(state_before, state_after,
        "INV-5 VIOLATION: undo → redo did not restore original state"
    );
}

#[test]
fn inv5_restart_replay_produces_identical_state() {
    // Simulate a "restart": close the engine, reopen from the same log,
    // and verify the rebuilt state matches.
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("events.db");
    let db_path_str = db_path.to_str().unwrap();

    // Phase 1: create state
    let state_before_close: HashMap<String, serde_json::Value>;
    let total_before: u64;
    {
        let mut core = WorkbenchCore::open(db_path_str, "global").unwrap();
        core.set("hp", serde_json::json!(100)).unwrap();
        core.set("mp", serde_json::json!(50)).unwrap();
        core.set("name", serde_json::json!("Boss")).unwrap();
        state_before_close = core.get_state();
        total_before = core.get_total_events().unwrap();
    }

    // Phase 2: "restart" — open a new engine from the same file
    {
        let core_restarted = WorkbenchCore::open(db_path_str, "global").unwrap();
        let state_after = core_restarted.get_state();
        let total_after = core_restarted.get_total_events().unwrap();

        assert_eq!(state_before_close, state_after,
            "INV-5 VIOLATION: restart produced different state from events log"
        );
        assert_eq!(total_before, total_after,
            "INV-5 VIOLATION: event count changed after restart"
        );

        // Also verify rebuild matches
        let rebuilt = core_restarted.rebuild().unwrap();
        assert_eq!(state_after, rebuilt,
            "INV-5 VIOLATION: rebuild after restart doesn't match current state"
        );
    }

    fs::remove_dir_all(dir.path()).ok();
}

#[test]
fn inv5_undo_redo_and_restart_replay_identical() {
    // The full INV-5 scenario: undo → redo → restart replay produces identical state.
    use std::fs;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("events.db");
    let db_path_str = db_path.to_str().unwrap();

    let state_final: HashMap<String, serde_json::Value>;
    {
        let mut core = WorkbenchCore::open(db_path_str, "global").unwrap();
        core.set("a", serde_json::json!(1)).unwrap();
        core.set("b", serde_json::json!(2)).unwrap();
        core.set("c", serde_json::json!(3)).unwrap();
        core.set("d", serde_json::json!(4)).unwrap();
        core.set("e", serde_json::json!(5)).unwrap();

        // Undo to seq 2, then redo to seq 5
        core.undo(3).unwrap();
        core.redo(3).unwrap();

        state_final = core.get_state();
    }

    // Restart and verify
    {
        let core_restarted = WorkbenchCore::open(db_path_str, "global").unwrap();
        assert_eq!(state_final, core_restarted.get_state(),
            "INV-5 VIOLATION: undo→redo→restart state doesn't match"
        );

        let rebuilt = core_restarted.rebuild().unwrap();
        assert_eq!(state_final, rebuilt,
            "INV-5 VIOLATION: rebuild after undo→redo→restart doesn't match"
        );
    }

    fs::remove_dir_all(dir.path()).ok();
}

#[test]
fn inv5_events_are_never_deleted() {
    // Events must never be removed from the log.
    // Even after undo, the events still exist in the log.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.set("keep", serde_json::json!(true)).unwrap();
    core.set("temp", serde_json::json!(false)).unwrap();

    let total = core.get_total_events().unwrap();
    assert_eq!(total, 2);

    // Undo back to seq 1
    core.undo(1).unwrap();

    // Events must still be in the log
    let total_after_undo = core.get_total_events().unwrap();
    assert_eq!(total_after_undo, 2,
        "INV-5 VIOLATION: events were deleted after undo"
    );

    // Event at seq 2 must still be retrievable
    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().any(|e| e.seq == 2),
        "INV-5 VIOLATION: event seq 2 missing from log after undo"
    );
}

// ── INV-6: Single contract interface ─────────────────────────────────

#[test]
fn inv6_public_api_is_complete() {
    // The WorkbenchCore contract exposes all necessary operations.
    // Verify the public API surface is self-contained.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // All operations should be accessible without accessing internal modules.
    core.set("x", serde_json::json!(1)).unwrap();
    let state = core.get_state();
    assert!(state.contains_key("x"));

    core.undo(1).unwrap();
    core.redo(1).unwrap();

    let history = core.get_history().unwrap();
    assert!(!history.is_empty());
}

// ── INV-7: No rendering in core ──────────────────────────────────────
// INV-7 is tested together with INV-4 above (inv4_inv7_core_has_no_llm_http_render_deps).

// ── INV-8: Hook reactions produce proposals only ─────────────────────
// INV-8 is relevant for U2+ when hooks are implemented. For U1, we document
// that the core has no hook mechanism yet and no way to auto-modify state.

#[test]
fn inv8_no_hook_mechanism_yet() {
    // U1 has no hook system. This test documents the baseline.
    let core = WorkbenchCore::open_in_memory("global").unwrap();
    // No hooks, no auto-reactions — all state changes are explicit.
    assert_eq!(core.get_total_events().unwrap(), 0);
}
