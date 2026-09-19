# complexity: handle_rpc

**Location:** `packages/tddy-session-lifecycle/src/connection_service/daemon_rpc_handler.rs` — `handle_rpc`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **147 lines** · **nesting 8** · 3 parameters · budget 60 / 4
**Thresholds breached:** length 147 > 60; nesting 8 > 4 (**2× the ceiling**)
**Restructure:** `extract_method --variant module`, one arm at a time
**Status:** Open — **unclaimed**

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-09-19 | 147 | 8 | 143 → 147 in PR #518 (the `Weak` upgrade for the `Arc` cycle) |

## What the tool found

A dispatch function that matches `(service, method)` and, inside each arm, decodes, calls and
re-encodes. Nesting 8 is the deepest in `tddy-session-lifecycle` — the arms nest match-inside-match
around `Result` handling.

PR #518 changed its `conn` field from `Arc<DaemonSessionHost>` to `Weak`, to break the reference
cycle that stopped every `test_service` daemon from dropping its jail registry. That added the
upgrade-or-refuse step, worth 4 lines and no extra nesting.

## What would close it

Each match arm is independently extractable — the body is a sequence of siblings, which is the
geometry `extract_method` handles best. Pulling even the three longest arms out would take both
length and nesting under budget without touching the dispatch shape.
