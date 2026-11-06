# oversized-file: codex.rs

**Location:** `packages/tddy-agent-backend/src/backend/codex.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 — `/pr-wrap` step 3.5 file-length gate on #798 (`#agent-tools` 9/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **606 production lines** · budget 500 · **1.2× over**
**Thresholds breached:** length 606 > 500
**Restructure:** not designed
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-03)
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 479 → 606 | pushed over budget by #798: `with_headless_renderer` and the spec-rendered `invoke_from_spec`, beside the legacy `build_codex_exec_argv` path |

## What would close it

Delete `build_codex_exec_argv` and the hand-built invoke path once that deletion is approved (`docs/dev/todo/2026-11-03-headless-others-legacy-argv-deletion.md`), or move the spec-rendered path into a `codex/from_spec.rs` sibling (engine-driven, `code-restructuring`).
