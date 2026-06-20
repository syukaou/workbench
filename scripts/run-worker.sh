#!/usr/bin/env bash
# WORKBENCH headless worker launcher.
#
# Runs exactly ONE `/work <card_id>` cycle as an autonomous, headless Claude
# Code worker: claim a ready Hermes-Kanban card -> read its spec -> implement
# in the card's git worktree -> gate (cargo test + ui build) -> commit & push
# the card branch -> complete the card -> stop.
#
# The worker runs with `--permission-mode bypassPermissions` so it can use
# tools without per-call approval. This is the Stage-1 flywheel worker. Its
# safety rests on FOUR guardrails (see docs/CLAUDE.md, docs/WORKFLOW.md):
#   1. worktree isolation (each card on its own branch/worktree)
#   2. the CI gate on every push/PR (.github/workflows/ci.yml)
#   3. the invariant tests INV-1..8 (core/src/invariant_tests.rs)
#   4. the /work rule: on any red-line conflict, `block` the card — never hack.
# It pushes a feature branch only; it never merges to main (that stays gated).
#
# Usage: scripts/run-worker.sh <card_id>
set -euo pipefail

ID="${1:?usage: run-worker.sh <card_id>}"

# Daemon-spawned shells get a minimal PATH; ensure the toolchain is reachable.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

cd "$HOME/projects/workbench"

exec claude -p "/work $ID" \
  --permission-mode bypassPermissions \
  --model claude-opus-4-8 \
  --output-format json
