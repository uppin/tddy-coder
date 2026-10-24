# oversized-file: mod.rs

**Location:** `packages/tddy-agent-backend/src/backend/mod.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **850 production lines** (2026-10-24; 877 at detection, first `#[cfg(test)]` then at line 878; two test modules with no production code after the first) · budget 500 · **1.7× over**
**Thresholds breached:** length 850 > 500
**Restructure:** not designed
**Status:** Open — partially fixed 2026-10-24 (906 → 850 by `#warp-agent` 1/2, #717, which moved `context_globs_for_agent` out); seams A–D remain — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/mod.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 877 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: 878 → 877, because Cut 1 folded the two `crate::workflow` imports into one `use tddy_workflow::{GoalHints, GoalId}`. `write_codex_thread_id_file` widened `pub(crate)` → `pub` on its re-export line (the workflow engine calls it) |
| 2026-10-22 | 879 | `#provider-route` 3/9 (#664): 877 → 879 — the `opencode` row of `context_globs_for_agent` (one comment, one arm). **Regressed, split deferred**: the file is stack-shared (`#provider-route` 4/9 #665 and 7/9 #668 also change it), and a split mid-stack would conflict in both; the follow-up is after the stack lands |
| 2026-10-22 | 880 | `#provider-route` 4/9 (#665): 879 → 880 — `mod routed;`. **Regressed, split deferred**: still stack-shared with `#provider-route` 7/9 (#668) |
| 2026-10-22 | 881 | `#provider-route` 6/9 (#667): 880 → 881 — the `muse` row of `context_globs_for_agent` (one arm; its comment replaced, not added). **Regressed, split deferred**: still stack-shared with `#provider-route` 7/9 (#668) |
| 2026-10-22 | 906 | `#provider-route` 7/9 (#668): 882 → 906 (882 is this count on the rebased base) — `AnyBackend::OpenCode` and its five dispatch arms, the `OpenCode` selection option, `OPENCODE_DEFAULT_MODEL` and its rows in `backend_from_label`, `default_model_for_agent`, `curated_models_for_agent` and `preselected_index_for_agent`, `mod opencode_acp;` / `mod acp_updates;` and the re-export. **Regressed, split deferred**: stack-shared — `#provider-route` 8/9 (#669) and 9/9 (#670) sit on this branch, and the split is the follow-up after the stack lands (seams B and C above take most of what this PR added) |
| 2026-10-24 | 850 | `#warp-agent` 1/2 (#717): 906 → 908 with the `warp` row of `context_globs_for_agent` (one comment, one arm), then 908 → 850 by moving `SHARED_CONTEXT_GLOBS` and `context_globs_for_agent` into `backend/context_globs.rs` (60 production lines) with `tddy-tools restructure` (`move_item`, then a named `pub use context_globs::context_globs_for_agent` so every path is kept). **Partially fixed**: still 1.7× over |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | child-process PID tracking: `CHILD_PID`, `set_child_pid`, `clear_child_pid`, `get_child_pid`, both `kill_child_process` variants, `format_command_for_log` (L158–241) | ~85 |
| B | the selection questions: `backend_selection_question`, `workflow_recipe_selection_question`, `recipe_cli_name_from_selection_label`, `backend_from_label`, `preselected_index_for_agent` (L513–617, L816–829) | ~120 |
| C | the model catalogue data: the Claude aliases and pinned models, `default_model_for_agent`, `BackendModel(s)`, `curated_models_for_agent`, `claude_cli_models`, `cursor_cli_models`, `acp_models_from_session_state` (L618–700, L761–815). `model_catalog.rs` is its natural home | ~140 |
| D | request and sink types: `AgentOutputSink`, `ProgressSink`, `SessionMode`, `RemoteToolEnv`, `InvokeRequest` (L283–493) | ~210 |

`context_globs_for_agent` is out (`backend/context_globs.rs`); seams A–C leave about 470 lines, under budget, and D takes it further. Line numbers above predate that move. `pub use` at the module root keeps every path. Prove the seams with `restructure check --deep` before applying them.
