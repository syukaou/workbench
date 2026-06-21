---
description: Claim and complete ONE Hermes Kanban card on the active board, then stop. Usage: /work [card_id]
---
You are a headless WORKBENCH dev worker. Board: the ambient Hermes board from `$HERMES_KANBAN_BOARD` (defaults to `workbench-mvp` when unset) — every `hermes kanban` call below targets it automatically, so do NOT pass `--board`. Do exactly ONE card this run, then STOP. Do not pick up a second card.

Card target: `$ARGUMENTS` — a card id like `t_xxxx`. If empty, pick the FIRST `ready` card from the board.

## Before anything: read the red lines
Read `docs/CLAUDE.md` (architecture invariants INV-1..INV-8) and obey them. NEVER silently violate a red line to make something work. If a card cannot be done without violating one, `block` it and stop.

## Cycle (one card)
1. **Identify**: if `$ARGUMENTS` is empty, run `hermes kanban list` and take the first card with status `ready`; else use `$ARGUMENTS`.
2. **Claim**: `hermes kanban claim <id> --ttl 3600`. Note the printed worktree path. If the claim fails (already held), STOP with `SKIP <id>: already claimed`.
3. **Read spec**: `hermes kanban show <id>` — the card body is the full, authoritative spec. `cd` into the worktree path from step 2.
4. **Implement** exactly what the body specifies — no scope creep, nothing extra. Match the surrounding code's style/idioms. If the change touches a state / AI / event / contract / render boundary, add or update the matching invariant test in the SAME change (CLAUDE.md §3).
5. **Heartbeat** on long work: `hermes kanban heartbeat <id>` at least hourly.
6. **Gate** (both must pass):
   - `cargo test` (from repo root — runs the workspace incl. `core/src/invariant_tests.rs`).
   - If anything under `ui/` changed: `cd ui && npm run build` (tsc + vite).
   - If finishing would require violating a red line: `hermes kanban block <id> "<one-line reason>"` and STOP with `BLOCKED <id>: <reason>`.
7. **Commit + push** the card's branch: `git add -A && git commit -m "<card title>" && git push -u origin HEAD`.
8. **Complete**: `hermes kanban complete <id>` with a handoff summary covering: `changed_files`, `verification` (the exact commands you ran and that they passed), `residual_risk` (anything untested / needing human review). No secrets in the handoff.
9. **STOP.** End your run with a single final line: `DONE <id>` or `BLOCKED <id>: <reason>` or `SKIP <id>: <reason>`.
