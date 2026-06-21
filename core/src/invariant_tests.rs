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

#[test]
fn inv1_undo_redo_cursor_is_single_source_for_can_undo_redo() {
    // core-as-truth (INV-1 / U0R UI refactor): the frontend keeps NO undo
    // stack of its own. It derives canUndo/canRedo solely from the core's
    // event-log cursor, exposed to WASM as
    //     undo_redo_status() = { current_seq, total_events }
    // with  canUndo = current_seq > 0  and  canRedo = current_seq < total_events.
    // Pinning that cursor contract here means the UI's single source of truth
    // can never silently drift from the event log.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Fresh log: nothing to undo or redo.
    assert_eq!(core.get_current_seq(), 0);
    assert_eq!(core.get_total_events().unwrap(), 0);

    core.create_node("a", "A").unwrap();
    core.create_node("b", "B").unwrap();
    core.create_node("c", "C").unwrap();
    assert_eq!(core.get_current_seq(), 3, "cursor must advance with each event");
    assert_eq!(core.get_total_events().unwrap(), 3);
    // At the tip: canUndo true, canRedo false.
    assert!(core.get_current_seq() > 0);
    assert!(!(core.get_current_seq() < core.get_total_events().unwrap()));

    // Undo two: the cursor rewinds, but events are never deleted (INV-5).
    core.undo(2).unwrap();
    assert_eq!(core.get_current_seq(), 1, "undo must move the cursor back");
    assert_eq!(
        core.get_total_events().unwrap(),
        3,
        "INV-1/INV-5: events are never deleted — only the cursor moves"
    );
    // Mid-history: both canUndo and canRedo are true.
    assert!(core.get_current_seq() > 0);
    assert!(core.get_current_seq() < core.get_total_events().unwrap());

    // Redo one: the materialized state the UI reads back must reflect exactly
    // the events up to the cursor (a, b present; c still folded out).
    core.redo(1).unwrap();
    assert_eq!(core.get_current_seq(), 2);
    let at_cursor = core.get_state();
    assert!(at_cursor.contains_key("node:a") && at_cursor.contains_key("node:b"));
    assert!(
        !at_cursor.contains_key("node:c"),
        "INV-1 VIOLATION: get_state() (the UI's source) shows an event past the cursor"
    );

    // Back to the root: canUndo false, canRedo true; state is empty again.
    core.undo_all().unwrap();
    assert_eq!(core.get_current_seq(), 0);
    assert!(core.get_current_seq() < core.get_total_events().unwrap());
    assert!(
        core.get_state().is_empty(),
        "INV-1: at cursor 0 the UI must see empty state, even though events remain in the log"
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

#[test]
fn inv3_proposal_preview_does_not_mutate_core() {
    // U2R overlay rule: a typed AI proposal may be previewed (rendered as a
    // pending overlay) WITHOUT touching the core. Only an explicit, per-command
    // accept writes the event log — one event per command. This is the runtime
    // guarantee behind the canvas "pending" preview.
    use crate::cli_bridge::parse_proposals;

    // The AI's typed output for "central hall + 3 branches + a one-way shortcut".
    let raw = r#"[
        {"CreateNode": {"node_id": "hall", "label": "Central Hall"}},
        {"CreateNode": {"node_id": "branch_a", "label": "Branch A"}},
        {"CreateNode": {"node_id": "branch_b", "label": "Branch B"}},
        {"CreateNode": {"node_id": "branch_c", "label": "Branch C"}},
        {"CreateEdge": {"from_node": "hall", "to_node": "branch_a", "bidirectional": true}},
        {"CreateEdge": {"from_node": "hall", "to_node": "branch_b", "bidirectional": true}},
        {"CreateEdge": {"from_node": "hall", "to_node": "branch_c", "bidirectional": true}},
        {"CreateEdge": {"from_node": "branch_a", "to_node": "branch_c", "bidirectional": false}},
        {"MarkNode": {"node_id": "hall", "mark": "spawn"}}
    ]"#;

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Parsing the proposal into typed commands is the overlay's job — it must
    // NOT write the core. The preview holds these commands; the log stays empty.
    let commands = parse_proposals(raw).unwrap();
    assert_eq!(commands.len(), 9);
    assert_eq!(
        core.get_total_events().unwrap(),
        0,
        "INV-3 VIOLATION: holding/previewing an AI proposal mutated the core"
    );
    assert!(
        core.get_state().is_empty(),
        "INV-3 VIOLATION: proposal preview leaked into core state before accept"
    );

    // Accept = dispatch each command through the contract, one event each.
    let mut expected = 0;
    for cmd in commands {
        core.execute_command(cmd).unwrap();
        expected += 1;
        assert_eq!(
            core.get_total_events().unwrap(),
            expected,
            "INV-3 VIOLATION: accepting a command did not append exactly one event"
        );
    }

    // After accepting, the topology is now committed core truth.
    let state = core.get_state();
    assert!(state.contains_key("node:hall"));
    assert!(state.contains_key("edge:hall->branch_a"));
    assert!(state.contains_key("edge:branch_a->branch_c"));
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

#[test]
fn inv4_inv7_core_source_has_no_external_io() {
    // The Cargo.toml scan above only catches forbidden *crate names*. It does
    // NOT catch deterministic-boundary violations written directly against std —
    // e.g. spawning an external LLM CLI via std::process, or binding a
    // network listener via std::net. Those concerns belong OUTSIDE core (the
    // app/sidecar layer), per INV-4. This test scans every .rs file under
    // core/src for those source-level patterns so a NEW violation — in a file
    // that does not even exist yet — still trips CI. It mirrors the source-scan
    // style of inv6_core_modules_not_publicly_reachable.
    use std::fs;

    // ── KNOWN-DEFERRED ALLOWLIST ─────────────────────────────────────
    // These are the deferred cli_bridge / cli_server INV-4 violations: the
    // external-LLM-CLI bridge (std::process spawn) and its local TCP
    // server (std::net) still live in core, tracked for migration OUT of core
    // to the app/sidecar layer. This allowlist MUST shrink to empty when that
    // migration lands, and NO new file may EVER be added to it — adding a file
    // here to silence the scan is itself a red-line breach.
    const ALLOWLIST: [&str; 2] = ["cli_bridge.rs", "cli_server.rs"];

    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");

    // Build the forbidden needles at runtime so the literal patterns do not
    // appear contiguously in THIS file, which is itself one of the scanned
    // sources — otherwise the guardrail would flag its own test code.
    //
    // External-process spawn is detected robustly across aliased / un-aliased /
    // grouped imports. cli_bridge.rs actually imports Command under an alias via
    // a grouped use of the std process module (Command + Output together) and
    // then calls the alias's constructor — so a contiguous "process" + "Command"
    // literal never appears. We instead require the std process import path AND a
    // constructor call in the same file. The AND keeps an incidental mention in
    // a comment from tripping the scan, while still catching both the grouped
    // form and a plain aliased `use ...Command as Foo; Foo::new(...)`.
    // (Needles are assembled from fragments so they do not appear contiguously
    // in this file, which is itself scanned.)
    let process_import = ["process", "::"].concat(); // the std process import path
    let constructor_call = ["::", "new("].concat(); // any alias's constructor call
    let tcp_listener = ["Tcp", "Listener"].concat();
    let udp_socket = ["Udp", "Socket"].concat();

    // Recursively collect every .rs file under core/src. INV-4 governs the whole
    // `core` crate ("无网络" in the core crate), so the scan must include nested
    // modules and the bin/ target, not just the top-level files — a violation
    // could hide in a subdirectory or a new binary just as easily.
    let mut rs_files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(src_dir)];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("core/src must be readable") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                rs_files.push(path);
            }
        }
    }

    for path in &rs_files {
        let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
        if ALLOWLIST.contains(&file_name.as_str()) {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();

        assert!(
            !(source.contains(&process_import) && source.contains(&constructor_call)),
            "INV-4 VIOLATION: {} imports + constructs the std::process spawn API \
             (external-process spawn) — the LLM-CLI bridge belongs OUTSIDE the \
             deterministic core, not in core/src",
            file_name
        );
        assert!(
            !source.contains(&tcp_listener) && !source.contains(&udp_socket),
            "INV-4 VIOLATION: {} binds a network listener (std::net) \
             — networking belongs OUTSIDE the deterministic core, not in core/src",
            file_name
        );
    }

    // ── META: prove the guardrail still bites (cannot rot into a no-op) ──
    // Exactly the two known-deferred files — silently widening the allowlist
    // (to exempt a new violator) fails the test.
    assert_eq!(
        ALLOWLIST.len(),
        2,
        "INV-4 allowlist must contain exactly the two known-deferred files"
    );
    assert!(
        ALLOWLIST.contains(&"cli_bridge.rs") && ALLOWLIST.contains(&"cli_server.rs"),
        "INV-4 allowlist must be exactly {{cli_bridge.rs, cli_server.rs}}"
    );
    // Each allowlisted file must still EXIST under core/src. Once the migration
    // moves a file out of core, this fails and FORCES dropping the now-stale
    // exemption — so the allowlist can only ever shrink toward empty.
    for name in ALLOWLIST {
        let p = std::path::Path::new(src_dir).join(name);
        assert!(
            p.exists(),
            "INV-4 allowlist names '{}' but it no longer exists under core/src — \
             the migration moved it; remove it from the allowlist",
            name
        );
    }
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

#[test]
fn inv5_node_undo_redo_reverts_materialized_state() {
    // M1 / INV-5: the contract undo/redo that the WASM bridge now exposes must
    // revert the materialized state by re-folding the event log. create_node
    // then undo leaves no node:<id> in get_state(); redo restores it; and
    // state == rebuild(events) throughout. This is the invariant the UI's
    // single-source-of-truth refactor (U2-rebuild) will rely on.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.create_node("x", "Room X").unwrap();
    assert!(
        core.get_state().contains_key("node:x"),
        "node:x should exist after create_node"
    );

    core.undo(1).unwrap();
    assert!(
        !core.get_state().contains_key("node:x"),
        "INV-5 VIOLATION: undo must remove node:x from the materialized state"
    );
    assert_eq!(core.get_current_seq(), 0);

    core.redo(1).unwrap();
    assert!(
        core.get_state().contains_key("node:x"),
        "redo must restore node:x"
    );

    let rebuilt = core.rebuild().unwrap();
    assert_eq!(
        core.get_state(),
        rebuilt,
        "INV-5: materialized state must equal rebuild(events) after undo/redo"
    );
}

#[test]
fn inv2_inv5_canvas_edits_are_logged_and_undoable() {
    // U1R: the topology canvas routes every structural edit through the core
    // (no second source of truth, no local snapshot stack). This pins the
    // three edits the canvas adds — RemoveNode / RemoveEdge / MarkNode — to the
    // event log: each appends exactly one event (INV-2), state always equals
    // rebuild(events) (INV-5), and a single undo reverses each edit.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    core.create_node("a", "A").unwrap();
    core.create_node("b", "B").unwrap();
    core.create_edge("a", "b", false).unwrap();
    let base = core.get_total_events().unwrap();

    // MarkNode → one event; the mark lands in the materialized state.
    core.mark_node("a", "spawn").unwrap();
    assert_eq!(
        core.get_total_events().unwrap(),
        base + 1,
        "INV-2 VIOLATION: MarkNode did not append exactly one event"
    );
    assert_eq!(
        core.get_state()["node:a"]["marks"],
        serde_json::json!(["spawn"]),
        "MarkNode must be visible in the state the canvas reads back"
    );

    // RemoveEdge → one event; edge:a->b gone from state.
    core.remove_edge("a", "b").unwrap();
    assert_eq!(core.get_total_events().unwrap(), base + 2);
    assert!(
        !core.get_state().contains_key("edge:a->b"),
        "RemoveEdge must drop the edge from the materialized state"
    );

    // RemoveNode → one event; node:a gone from state.
    core.remove_node("a").unwrap();
    assert_eq!(core.get_total_events().unwrap(), base + 3);
    assert!(
        !core.get_state().contains_key("node:a"),
        "RemoveNode must drop the node from the materialized state"
    );

    // INV-5: materialized state == rebuild(events) at the tip.
    assert_eq!(
        core.get_state(),
        core.rebuild().unwrap(),
        "INV-5 VIOLATION: canvas edits made state diverge from the event log"
    );

    // The toolbar Undo is core-true: one undo reverses exactly the last edit
    // (RemoveNode), restoring node:a — events are never deleted (INV-5).
    core.undo(1).unwrap();
    assert!(
        core.get_state().contains_key("node:a"),
        "INV-5 VIOLATION: undo must restore the node removed by the canvas"
    );
    assert_eq!(
        core.get_total_events().unwrap(),
        base + 3,
        "INV-5 VIOLATION: undo must move the cursor, never delete events"
    );
    assert_eq!(
        core.get_state(),
        core.rebuild_up_to(core.get_current_seq()).unwrap(),
        "INV-5: state after undo must equal rebuild up to the cursor"
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
    // INV-6: the deterministic core's internals — engine (Engine) and the
    // event store (log::EventStore / memory_store::MemoryStore) — must NOT be
    // part of the public API. If they were, a consumer could construct an
    // Engine or EventStore directly and bypass WorkbenchCore, defeating the
    // single contract boundary.
    //
    // We enforce this structurally by scanning lib.rs (same approach as the
    // INV-3/4/7/8 source-scan guardrails). A future `pub mod engine` or
    // `pub use log::EventStore` makes this test fail — the guardrail actually
    // bites, instead of merely asserting on a comment.
    let lib_rs = include_str!("lib.rs");

    let forbidden_exposures = [
        "pub mod engine",
        "pub mod log",
        "pub mod memory_store",
        "pub use log::EventStore",
        "pub use memory_store",
        "pub use engine::Engine",
    ];
    for pattern in &forbidden_exposures {
        assert!(
            !lib_rs.contains(pattern),
            "INV-6 VIOLATION: lib.rs exposes a core internal via '{}' — consumers could bypass WorkbenchCore",
            pattern
        );
    }

    // The contract type and the typed Command surface (needed for the public
    // execute_command signature) MUST be exported — that is the boundary.
    assert!(
        lib_rs.contains("pub use contract::WorkbenchCore"),
        "INV-6: WorkbenchCore must be exported as the single contract boundary"
    );
    assert!(
        lib_rs.contains("pub use engine::Command"),
        "INV-6: Command must be re-exported while the engine module stays private"
    );

    // Runtime: every consumer operation works through the contract alone,
    // with no direct engine/store access.
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();
    core.set("a", serde_json::json!(1)).unwrap();
    core.set("b", serde_json::json!(2)).unwrap();

    let event = core.create_node("room1", "Central Hall").unwrap();
    assert_eq!(event.event_type, EventType::NodeCreated);

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

// ── INV-5 / INV-6: Save / Load goes through the event log ────────────
//
// U4R: project save/load must NOT smuggle in a second source of truth. Save
// exports the event log (export_snapshot), load replays those events through
// import_snapshot — the projection is rebuilt by folding the log, never copied
// in wholesale. This pins three things: (1) the saved `state` field is exactly
// fold(events) so a viewer can trust it, (2) loading into a fresh core via the
// serialized events reproduces the identical state, and (3) the imported log is
// live history — undo still walks it. The UI coordinate layer is intentionally
// absent here: it is view-only and never enters the event log.

#[test]
fn inv5_inv6_save_load_roundtrips_through_event_log() {
    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // Build a non-trivial project: nodes, an edge, a mark, a POI.
    core.create_node("entrance", "Entrance").unwrap();
    core.create_node("vault", "Vault").unwrap();
    core.create_edge("entrance", "vault", false).unwrap();
    core.mark_node("entrance", "spawn").unwrap();
    core.attach_poi("vault", "locked-chest", None).unwrap();

    let saved_state = core.get_state();
    let saved_events = core.get_history().unwrap();

    // ── Save: export_snapshot is the sole serialized form ────────────
    let snapshot = core.export_snapshot().unwrap();
    // The materialized `state` in the file must equal fold(events): a consumer
    // reading the snapshot's state is reading the projection, not a side copy.
    assert_eq!(
        snapshot["state"],
        serde_json::to_value(&saved_state).unwrap(),
        "INV-5 VIOLATION: snapshot.state diverged from the materialized state"
    );
    assert_eq!(
        snapshot["events"].as_array().unwrap().len(),
        saved_events.len(),
        "snapshot must carry every event in the log"
    );

    // ── Load: a fresh core rebuilds purely by replaying the events ───
    // Round-trip the events through JSON exactly as the file on disk would,
    // so this exercises the real save→file→load deserialization path.
    let events: Vec<crate::Event> =
        serde_json::from_value(snapshot["events"].clone()).unwrap();
    let mut loaded = WorkbenchCore::open_in_memory("global").unwrap();
    loaded.import_snapshot(&events).unwrap();

    // State after load == saved state, and it equals fold(events) in the new
    // core — load did not bypass the log (INV-5/INV-6).
    assert_eq!(
        loaded.get_state(),
        saved_state,
        "INV-6 VIOLATION: loaded state != saved state"
    );
    assert_eq!(
        loaded.get_state(),
        loaded.rebuild().unwrap(),
        "INV-5 VIOLATION: loaded state must equal rebuild(events) in the new core"
    );
    assert_eq!(
        loaded.get_history().unwrap(),
        saved_events,
        "load must preserve the event log verbatim (append-only, INV-5)"
    );

    // ── The imported log is live history: undo still walks it ────────
    // import places the cursor at the tip; undo of the last event (AttachPOI)
    // detaches the POI, proving the load did not freeze a flat snapshot.
    assert_eq!(
        loaded.get_current_seq(),
        loaded.get_total_events().unwrap(),
        "import must leave the undo cursor at the tip of the imported log"
    );
    loaded.undo(1).unwrap();
    assert_eq!(
        loaded.get_state()["node:vault"]["pois"],
        serde_json::json!([]),
        "INV-5 VIOLATION: undo on the imported log must reverse the last event"
    );
    assert_eq!(
        loaded.get_state(),
        loaded.rebuild_up_to(loaded.get_current_seq()).unwrap(),
        "INV-5: state after undo must equal rebuild up to the cursor"
    );
}

// ── MVP acceptance: SPEC §4.3 full loop, end to end ──────────────────

#[test]
fn mvp_acceptance_spec_4_3_end_to_end() {
    // The SPEC §4.3 acceptance loop run entirely through the WorkbenchCore
    // contract: AI proposal (parsed, not yet accepted) → accept → topology
    // appears → human adds a room → defines a Boss and binds a POI → undo/redo
    // any step → save + reload persists. This ties the per-feature guardrails
    // (INV-3 / INV-5 / INV-6) into the single acceptance the SPEC defines.

    let mut core = WorkbenchCore::open_in_memory("global").unwrap();

    // 1. AI proposal: "central hall + three branches + one one-way shortcut".
    //    Parsing the typed proposal must NOT write the core (INV-3).
    let proposal = r#"[
        {"CreateNode": {"node_id": "central", "label": "中央大厅"}},
        {"CreateNode": {"node_id": "branch_a", "label": "支线A"}},
        {"CreateNode": {"node_id": "branch_b", "label": "支线B"}},
        {"CreateNode": {"node_id": "branch_c", "label": "支线C"}},
        {"CreateEdge": {"from_node": "central", "to_node": "branch_a", "bidirectional": true}},
        {"CreateEdge": {"from_node": "central", "to_node": "branch_b", "bidirectional": true}},
        {"CreateEdge": {"from_node": "central", "to_node": "branch_c", "bidirectional": true}},
        {"CreateEdge": {"from_node": "branch_a", "to_node": "branch_c", "bidirectional": false}},
        {"MarkNode": {"node_id": "central", "mark": "spawn"}}
    ]"#;
    let commands = crate::cli_bridge::parse_proposals(proposal).unwrap();
    assert_eq!(commands.len(), 9);
    assert_eq!(
        core.get_total_events().unwrap(),
        0,
        "INV-3: parsing an AI proposal must not write the core before acceptance"
    );

    // 2. Accept → execute each proposed command through the sole write path.
    for cmd in commands {
        core.execute_command(cmd).unwrap();
    }
    let st = core.get_state();
    assert!(st.contains_key("node:central"));
    assert!(
        !st["edge:branch_a->branch_c"]["bidirectional"]
            .as_bool()
            .unwrap(),
        "the one-way shortcut must survive accept"
    );
    assert!(st["node:central"]["marks"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("spawn")));

    // 3. Human extends: add a room manually.
    core.create_node("vault", "宝库").unwrap();

    // 4. Define a Boss, fill a value, attach a POI that references it.
    core.create_entity_type("Boss").unwrap();
    core.create_entity_instance("Boss", "dragon").unwrap();
    core.set_entity_field("dragon", "hp", serde_json::json!(5000))
        .unwrap();
    core.attach_poi("vault", "poi_boss", Some("dragon")).unwrap();
    let st = core.get_state();
    assert_eq!(st["node:vault"]["pois"][0]["entity_ref"], "dragon");
    assert_eq!(st["entity_instance:dragon"]["fields"]["hp"], 5000);

    // 5. Any step is undoable (re-fold from the log), then redoable.
    let seq_before = core.get_current_seq();
    core.undo(1).unwrap(); // reverse the POI attach
    assert_eq!(
        core.get_state()["node:vault"]["pois"],
        serde_json::json!([]),
        "INV-5: undo must reverse the POI attach via the event log"
    );
    core.redo(1).unwrap();
    assert_eq!(core.get_current_seq(), seq_before);
    assert_eq!(
        core.get_state()["node:vault"]["pois"][0]["poi_id"],
        "poi_boss"
    );

    // 6. Save → reload (simulated restart): data persists, rebuilt from the log.
    let saved = core.get_history().unwrap();
    let final_state = core.get_state();
    let mut reloaded = WorkbenchCore::open_in_memory("global").unwrap();
    reloaded.import_snapshot(&saved).unwrap();
    assert_eq!(
        reloaded.get_state(),
        final_state,
        "reload must restore the exact state"
    );
    assert_eq!(
        reloaded.get_state(),
        reloaded.rebuild().unwrap(),
        "INV-5: reloaded state == fold(events)"
    );
}
