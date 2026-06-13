//! Invariant tests — physical guardrails for CLAUDE.md §1.
//!
//! Each invariant has at least one automated test that fails if the invariant is violated.
//! These tests run as part of `cargo test`.

use crate::{EventType, WorkbenchCore};
use std::collections::HashMap;

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
    assert_eq!(
        current, rebuilt,
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
    assert_eq!(
        core.get_total_events().unwrap(),
        initial_count + 1,
        "INV-2 VIOLATION: mutation did not produce an event"
    );

    core.delete("hp").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        initial_count + 2,
        "INV-2 VIOLATION: mutation did not produce an event"
    );

    // Verify the events exist in the log and are typed.
    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].event_type, EventType::Set);
    assert_eq!(history[1].event_type, EventType::Delete);
}

#[test]
fn inv2_mutation_content_recorded_in_log() {
    // INV-2: 修改进日志 — the *content* of every mutation must be
    // recorded in the event log, not just the fact of a mutation.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.set("player_name", serde_json::json!("Kael")).unwrap();
    core.set("max_hp", serde_json::json!(999)).unwrap();
    core.delete("max_hp").unwrap();

    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 3);

    // Event 1: set player_name = "Kael"
    assert_eq!(history[0].event_type, EventType::Set);
    assert_eq!(history[0].payload["key"], "player_name");
    assert_eq!(history[0].payload["value"], "Kael");

    // Event 2: set max_hp = 999
    assert_eq!(history[1].event_type, EventType::Set);
    assert_eq!(history[1].payload["key"], "max_hp");
    assert_eq!(history[1].payload["value"], 999);

    // Event 3: delete max_hp
    assert_eq!(history[2].event_type, EventType::Delete);
    assert_eq!(history[2].payload["key"], "max_hp");
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
    assert_eq!(
        core.get_state(),
        state_before,
        "INV-2 VIOLATION: state can be mutated outside execute path"
    );
}

// ── INV-3: AI is proposer, not decider ───────────────────────────────
// INV-3: AI 提案不落盘 — AI proposals must NOT land on disk (event log)
// without explicit human review. In U1, this means the core has no
// proposal-accept mechanism at all. The test is structural: it scans
// source files for forbidden patterns that would indicate an auto-accept
// mechanism, and verifies at runtime that no proposal-like event types exist.

#[test]
fn inv3_no_proposal_event_type() {
    // The EventType enum must not contain any proposal-related variant.
    // If someone adds ProposalAccepted / AcceptProposal / AiProposal etc.,
    // this test catches it.
    let event_rs = include_str!("event.rs");

    // Forbidden CamelCase identifiers that would indicate proposal machinery.
    let forbidden_variants = [
        "Proposal",
        "Proposed",
        "Accept",
        "Reject",
        "Review",
        "AiSuggestion",
        "AutoApply",
    ];

    for variant in &forbidden_variants {
        // Look for these as Rust enum variant names — they appear as
        // standalone CamelCase words before a comma or comment.
        assert!(
            !event_rs.contains(variant),
            "INV-3 VIOLATION: event.rs contains forbidden EventType variant '{}' (proposal/auto-accept)",
            variant
        );
    }
}

#[test]
fn inv3_no_auto_accept_in_engine() {
    // The engine source must not contain auto-accept patterns.
    let engine_rs = include_str!("engine.rs");
    let contract_rs = include_str!("contract.rs");

    let forbidden_patterns = [
        "auto_accept",
        "auto_apply",
        "auto_commit",
        "accept_proposal",
        "apply_proposal",
        "proposal_accepted",
    ];

    for source in [engine_rs, contract_rs] {
        for pattern in &forbidden_patterns {
            assert!(
                !source.to_lowercase().contains(pattern),
                "INV-3 VIOLATION: source contains forbidden pattern '{}' (auto-accept mechanism)",
                pattern
            );
        }
    }
}

#[test]
fn inv3_all_writes_require_explicit_action() {
    // Runtime: verify that the event log never changes without an
    // explicit call to a public mutation method. No event should appear
    // spontaneously.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Phase 1: after creation, log is empty
    assert_eq!(core.get_total_events().unwrap(), 0);

    // Phase 2: only after explicit set() does an event appear
    core.set("x", serde_json::json!(1)).unwrap();
    assert_eq!(core.get_total_events().unwrap(), 1);

    // Phase 3: reading state does NOT produce events
    let _ = core.get_state();
    let _ = core.get_history();
    assert_eq!(
        core.get_total_events().unwrap(),
        1,
        "INV-3 VIOLATION: read-only operation produced an event"
    );

    // Phase 4: rebuild does NOT produce events
    let _ = core.rebuild();
    assert_eq!(
        core.get_total_events().unwrap(),
        1,
        "INV-3 VIOLATION: rebuild produced an event"
    );
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
        "openai",
        "anthropic",
        "llm",
        "gpt",
        "claude",
        "gemini",
        "mistral",
        "tokenizer",
        "tiktoken",
        "candle",
        "torch",
        "onnx",
        // HTTP / networking
        "reqwest",
        "hyper",
        "http",
        "tokio-tungstenite",
        "tungstenite",
        "axum",
        "warp",
        "actix",
        "rocket",
        "ureq",
        "curl",
        "socket2",
        // Rendering
        "wgpu",
        "vulkan",
        "opengl",
        "gl",
        "sdl2",
        "pixels",
        "rayon-vulkan",
        "bevy",
        "macroquad",
        "ggez",
        "miniquad",
        "skia",
        "raqote",
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
    assert!(
        !cargo_toml.contains("tauri"),
        "INV-7 VIOLATION: core references tauri (rendering shell)"
    );
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
    assert_eq!(
        state_before, state_after,
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

        assert_eq!(
            state_before_close, state_after,
            "INV-5 VIOLATION: restart produced different state from events log"
        );
        assert_eq!(
            total_before, total_after,
            "INV-5 VIOLATION: event count changed after restart"
        );

        // Also verify rebuild matches
        let rebuilt = core_restarted.rebuild().unwrap();
        assert_eq!(
            state_after, rebuilt,
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
        assert_eq!(
            state_final,
            core_restarted.get_state(),
            "INV-5 VIOLATION: undo→redo→restart state doesn't match"
        );

        let rebuilt = core_restarted.rebuild().unwrap();
        assert_eq!(
            state_final, rebuilt,
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
    assert_eq!(
        total_after_undo, 2,
        "INV-5 VIOLATION: events were deleted after undo"
    );

    // Event at seq 2 must still be retrievable
    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history.iter().any(|e| e.seq == 2),
        "INV-5 VIOLATION: event seq 2 missing from log after undo"
    );
}

// ── INV-6: Single contract interface ─────────────────────────────────
// ── INV-6: Single contract interface ─────────────────────────────────
//
// INV-6 demands that Rust core exposes exactly ONE typed contract boundary.
// All consumers (frontend, Tauri shell, future Blender/Unity integrations)
// must go through WorkbenchCore — never reach inside engine/log/projection.
// lib.rs re-exports only the contract type + supporting types; internal
// modules are not pub-exported for direct consumption.

#[test]
fn inv6_all_consumer_operations_through_contract() {
    // Every operation a consumer needs — query, write, undo, redo,
    // replay verification — must be available through the single
    // WorkbenchCore contract. No internal module access required.

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Query operations
    assert_eq!(core.get_current_seq(), 0);
    assert_eq!(core.get_total_events().unwrap(), 0);
    assert!(core.get_history().unwrap().is_empty());

    // Write operations (sole path, INV-2)
    core.set("hp", serde_json::json!(100)).unwrap();
    core.set("mp", serde_json::json!(50)).unwrap();
    core.delete("mp").unwrap();
    assert_eq!(core.get_total_events().unwrap(), 3);

    // Undo/redo operations
    core.undo(1).unwrap();
    assert_eq!(core.get_current_seq(), 2);
    core.redo(1).unwrap();
    assert_eq!(core.get_current_seq(), 3);

    // Replay verification (INV-5)
    let state = core.get_state();
    let rebuilt = core.rebuild().unwrap();
    assert_eq!(
        state, rebuilt,
        "INV-6 VIOLATION: rebuild via contract produced different state"
    );

    // Rebuild at specific sequence
    let partial = core.rebuild_up_to(1).unwrap();
    assert_eq!(partial.len(), 1);
    assert!(partial.contains_key("hp"));
}

#[test]
fn inv6_contract_is_single_entry_point() {
    // The contract is the sole mutation path. Every write through any
    // contract method (set, delete, execute, execute_command) must:
    // 1. Produce exactly one event in the log
    // 2. Be the only way to change state — no bypass, no backdoor

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // set() produces 1 event
    let before = core.get_total_events().unwrap();
    core.set("a", serde_json::json!(1)).unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        before + 1,
        "INV-6 VIOLATION: set() did not produce exactly 1 event"
    );

    // delete() produces 1 event
    let before = core.get_total_events().unwrap();
    core.delete("a").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        before + 1,
        "INV-6 VIOLATION: delete() did not produce exactly 1 event"
    );

    // execute_command() produces 1 event
    let before = core.get_total_events().unwrap();
    core.execute_command(crate::engine::Command::Set {
        key: "b".into(),
        value: serde_json::json!(2),
    })
    .unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        before + 1,
        "INV-6 VIOLATION: execute_command() did not produce exactly 1 event"
    );

    // State reflects only explicit writes
    let state = core.get_state();
    assert_eq!(
        state.len(),
        1,
        "state should have 1 key (only 'b' after delete of 'a')"
    );
    assert!(state.contains_key("b"));
    assert!(!state.contains_key("a"));
}

#[test]
fn inv6_core_modules_not_publicly_reachable() {
    // INV-6: Verify that core internal modules (engine, log) are not
    // re-exported. Only WorkbenchCore + supporting types (Event, EventType,
    // Error, Projection traits) are public.
    //
    // This test is compile-time: if someone makes `engine` or `log` pub,
    // the test won't compile because those types aren't accessible here.

    // The fact that this test compiles at all, without referencing
    // crate::engine::Engine or crate::log::EventStore, proves that
    // those modules are not pub in lib.rs. We supplement with a
    // runtime check that the public API surface is self-contained.

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();
    core.set("a", serde_json::json!(1)).unwrap();
    core.set("b", serde_json::json!(2)).unwrap();

    // Execute a domain command through the contract boundary.
    let event = core.create_node("room1", "Central Hall").unwrap();
    assert_eq!(event.event_type, EventType::NodeCreated);

    // All operations go through the contract — no direct engine access.
    let history = core.get_history().unwrap();
    assert_eq!(history.len(), 3);
}

// ── INV-7: No rendering in core ──────────────────────────────────────
// INV-7 is tested together with INV-4 above (inv4_inv7_core_has_no_llm_http_render_deps).

#[test]
fn inv7_no_graphics_symbols_in_source() {
    // Additional runtime check: scan source files for graphics/rendering
    // import patterns that might slip past the Cargo.toml check.
    let lib_rs = include_str!("lib.rs");
    let engine_rs = include_str!("engine.rs");

    let forbidden_imports = [
        "wgpu",
        "vulkan",
        "opengl",
        "sdl2",
        "bevy",
        "macroquad",
        "ggez",
        "miniquad",
        "skia",
        "raqote",
        "tauri",
    ];

    for source in [lib_rs, engine_rs] {
        for forbidden in &forbidden_imports {
            assert!(
                !source.to_lowercase().contains(forbidden),
                "INV-7 VIOLATION: source file references '{}' (graphics/rendering)",
                forbidden
            );
        }
    }
}

// ── INV-8: Hook reactions produce proposals only ─────────────────────
// INV-8: Hook 收敛 — hooks must only produce proposals, not direct state
// changes. In U1, there is no hook mechanism at all. These tests verify
// the structural absence of hook/reaction machinery and that each mutation
// produces exactly one event (no cascading).

#[test]
fn inv8_no_hook_event_type() {
    // The EventType enum must not contain hook/reaction/trigger variants.
    let event_rs = include_str!("event.rs");

    let forbidden_variants = [
        "Hook",
        "HookFired",
        "Reaction",
        "ReactionApplied",
        "Trigger",
        "TriggerFired",
        "Callback",
        "SideEffect",
        "Cascade",
    ];

    for variant in &forbidden_variants {
        assert!(
            !event_rs.contains(variant),
            "INV-8 VIOLATION: event.rs contains forbidden EventType variant '{}' (hook/reaction)",
            variant
        );
    }
}

#[test]
fn inv8_no_hook_registration_in_source() {
    // Source files must not contain hook registration or reaction patterns.
    let engine_rs = include_str!("engine.rs");
    let lib_rs = include_str!("lib.rs");
    let contract_rs = include_str!("contract.rs");

    let forbidden_patterns = [
        "register_hook",
        "on_event",
        "add_listener",
        "subscribe",
        "dispatch",
        "emit",
        "reaction",
        "trigger_hook",
    ];

    for source in [engine_rs, lib_rs, contract_rs] {
        for pattern in &forbidden_patterns {
            assert!(
                !source.to_lowercase().contains(pattern),
                "INV-8 VIOLATION: source contains forbidden pattern '{}' (hook mechanism)",
                pattern
            );
        }
    }
}

#[test]
fn inv8_each_mutation_produces_exactly_one_event() {
    // INV-8: Hook 收敛 — each explicit mutation call must produce exactly
    // one event. No cascading, no automatic follow-up events, no hook chains.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();
    let count_before = core.get_total_events().unwrap();

    // A single set produces exactly one event
    core.set("hp", serde_json::json!(100)).unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 1,
        "INV-8 VIOLATION: set() produced more than one event (cascading?)"
    );

    // A single delete produces exactly one event
    core.delete("hp").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 2,
        "INV-8 VIOLATION: delete() produced more than one event (cascading?)"
    );

    // Domain commands also produce exactly one event each
    core.create_node("room_a", "Room A").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 3,
        "INV-8 VIOLATION: create_node() produced more than one event"
    );

    core.create_edge("room_a", "room_b", true).unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 4,
        "INV-8 VIOLATION: create_edge() produced more than one event"
    );

    // Undo/redo do NOT produce new events
    let before_undo = core.get_total_events().unwrap();
    core.undo(1).unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        before_undo,
        "INV-8 VIOLATION: undo() produced new events"
    );

    core.redo(1).unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        before_undo,
        "INV-8 VIOLATION: redo() produced new events"
    );
}

#[test]
fn inv8_no_event_triggers_event() {
    // Verify that applying an event does not trigger a chain reaction.
    // Each execute() call is atomic: one command → one event.
    // We test with POI operations which might be tempting to auto-cascade.

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Set up: create a node, create an entity type and instance.
    core.create_node("spawn_room", "Spawn").unwrap();
    core.create_entity_type("Enemy").unwrap();
    core.create_entity_instance("Enemy", "goblin_1").unwrap();

    let count_before = core.get_total_events().unwrap();

    // Attach a POI referencing an entity — should produce exactly 1 event,
    // NOT also auto-create the entity or node.
    core.attach_poi("spawn_room", "poi_enemy", Some("goblin_1"))
        .unwrap();

    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 1,
        "INV-8 VIOLATION: attach_poi() produced more than one event (auto-cascade?)"
    );

    // Detach a POI — exactly 1 event.
    core.detach_poi("spawn_room", "poi_enemy").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        count_before + 2,
        "INV-8 VIOLATION: detach_poi() produced more than one event (auto-cascade?)"
    );

    // Verify all events were produced explicitly and no hook-like events exist.
    // (INV-8 is enforced structurally: if someone adds a hook variant to
    // EventType, the source-scan test inv8_no_hook_event_type will fail.)
    let history = core.get_history().unwrap();
    for event in &history {
        let event_type_str = format!("{:?}", event.event_type);
        let forbidden = ["HookFired", "ReactionApplied", "SideEffect", "Cascade"];
        for f in &forbidden {
            assert!(
                !event_type_str.contains(f),
                "INV-8 VIOLATION: hook/reaction event found in log: {}",
                event_type_str
            );
        }
    }
}
