# oversized-file: mod.rs

**Location:** `packages/tddy-agent-backend/src/backend/mod.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **877 production lines** of 1,366 total (first `#[cfg(test)]` at line 878; two test modules, L878–1160 and L1162–1366, with no production code after the first) · budget 500 · **1.8× over**
**Thresholds breached:** length 877 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-10-22 (877 → 881 across `#provider-route` 3/9, 4/9 and 6/9, stack-shared, split deferred) — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/mod.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 877 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: 878 → 877, because Cut 1 folded the two `crate::workflow` imports into one `use tddy_workflow::{GoalHints, GoalId}`. `write_codex_thread_id_file` widened `pub(crate)` → `pub` on its re-export line (the workflow engine calls it) |
| 2026-10-22 | 879 | `#provider-route` 3/9 (#664): 877 → 879 — the `opencode` row of `context_globs_for_agent` (one comment, one arm). **Regressed, split deferred**: the file is stack-shared (`#provider-route` 4/9 #665 and 7/9 #668 also change it), and a split mid-stack would conflict in both; the follow-up is after the stack lands |
| 2026-10-22 | 880 | `#provider-route` 4/9 (#665): 879 → 880 — `mod routed;`. **Regressed, split deferred**: still stack-shared with `#provider-route` 7/9 (#668) |
| 2026-10-22 | 881 | `#provider-route` 6/9 (#667): 880 → 881 — the `muse` row of `context_globs_for_agent` (one arm; its comment replaced, not added). **Regressed, split deferred**: still stack-shared with `#provider-route` 7/9 (#668) |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | child-process PID tracking: `CHILD_PID`, `set_child_pid`, `clear_child_pid`, `get_child_pid`, both `kill_child_process` variants, `format_command_for_log` (L158–241) | ~85 |
| B | the selection questions: `backend_selection_question`, `workflow_recipe_selection_question`, `recipe_cli_name_from_selection_label`, `backend_from_label`, `preselected_index_for_agent` (L513–617, L816–829) | ~120 |
| C | the model catalogue data: the Claude aliases and pinned models, `default_model_for_agent`, `BackendModel(s)`, `curated_models_for_agent`, `claude_cli_models`, `cursor_cli_models`, `acp_models_from_session_state` (L618–700, L761–815). `model_catalog.rs` is its natural home | ~140 |
| D | request and sink types: `AgentOutputSink`, `ProgressSink`, `SessionMode`, `RemoteToolEnv`, `InvokeRequest` (L283–493) | ~210 |

Seams A–C leave about 530 lines, so D, or `context_globs_for_agent` (L701–760), is needed too. `pub use` at the module root keeps every path. Prove the seams with `restructure check --deep` before applying them.
