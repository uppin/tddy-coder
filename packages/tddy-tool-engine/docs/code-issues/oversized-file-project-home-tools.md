# oversized-file: project_home_tools.rs

**Location:** `packages/tddy-tool-engine/src/project_home_tools.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #750 (`#start-parity` 1/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **709 production lines** (2026-10-28) · budget 500 · 1.4× over
**Thresholds breached:** length 699 > 500
**Restructure:** not designed
**Status:** Open — split deferred: the file is stack-shared (later `#start-parity` nodes add tools to it), and a split mid-stack would conflict in every one; the follow-up is after the stack lands — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 500 | `#start-parity` 1/9 (#750): 425 → 500 — `ListedSession.daemon_instance_id`, `ListProjectSessionsResult`, `UnreachableHost`, `TARGET_DAEMON_INSTANCE_ID`, `target_daemon_instance_id`, `is_project_home_target_tool`, `target_host_property`, `ProjectHomePort::list_peer_sessions` and the merged listing. **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 457 | `#start-parity` 3/9 (#752): 435 at its base (`#start-parity` 2/9 tip, measured with `/pr-wrap` 3.5's `awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++} END {print n+0}'`) → 457 (+22) — the `ListWorktrees`/`GetWorktree` names in `is_project_home_tool` and `is_project_home_target_tool`, `ProjectHomePort::list_worktrees`, the two dispatch arms, and `tool_def` made `pub(crate)`; the tools themselves live in the new `project_home_worktree_tools.rs` (411). Under budget by this measure; still stack-shared, so no split |
| 2026-10-25 | 558 | **regressed.** `#start-parity` 4/9 (#753): 457 → 558 (+101) — the `StartSession` tool now answers with a handle, and `GetSessionStart` / `CancelSessionStart` (definitions, `ProjectHomePort::{get_session_start, cancel_session_start}`, `StartIdArgs` and their two handlers); the ledger itself is the new `start_ledger.rs` (414). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 601 | **regressed.** `#start-parity` 5/9 (#754): 558 → 601 (+43) — `DescribeSessionStartOptions` in `is_project_home_tool` and `is_project_home_target_tool`, `ProjectHomePort::start_options`, its dispatch arm and handler; the definition, query and filters are the new `start_options.rs` (206). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 632 | **regressed.** `#start-parity` 6/9 (#755): 601 → 632 (+31) — `ReadSessionActivity` / `ReadSessionTranscript` in the module table, `is_project_home_tool` and `is_project_home_target_tool`, `ProjectHomePort::{session_activity, session_transcript, session_tool_call}` and the dispatch arm; the tools themselves are the new `session_follow.rs` (376). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 643 | **regressed.** `#start-parity` 7/9 (#756): 632 → 643 (+11) — `TerminateSession` in the module table, `is_project_home_tool` and `is_project_home_target_tool`, `ProjectHomePort::terminate_session`, the catalogue entry and the dispatch arm; the tool itself is the new `project_home_terminate.rs` (62). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-26 | 660 | **regressed.** `#start-parity` 8/9 (#757): 643 → 660 (+17) — `RefreshWorktreeDiskUsage` in the module table, `is_project_home_tool` and `is_project_home_target_tool`, `ProjectHomePort::refresh_worktree_disk_usage`, the catalogue entry and the dispatch arm; the tool itself is the new `project_home_disk_refresh.rs` (80). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-26 | 699 | **regressed.** `#start-parity` 9/9 (#758): 660 at its base (`#start-parity` 8/9 tip) → 699 (+39) — `SessionToolRole`, `is_conversation_spawner_tool` and `session_offered_catalog_for` (a conversation spawner's catalogue: the daemon-wide one plus the four start tools). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-28 | 709 | **regressed.** `#home-inbox` 2/8 (#767): 699 → 709 (+10) — `ReadSessionTerminal` in the module table, `is_project_home_tool`, `is_project_home_target_tool`, the catalogue entry and a dispatch arm that names the wiring mistake; the tool itself is the new `project_home_terminal_tools.rs` (114). **Split deferred** (stack-shared: `#home-inbox` 5/8 and 6/8 touch the file) |

## What would close it — candidate seams (not proven)

| Seam | Items |
|---|---|
| A | the target-host argument: `TARGET_DAEMON_INSTANCE_ID`, `target_daemon_instance_id`, `is_project_home_target_tool`, `target_host_property` → `project_home_target_host.rs` |
| B | the tool catalogue: `project_home_tool_defs` and its schema helpers |
| C | the listing types: `ListedSession`, `ListProjectSessionsResult`, `UnreachableHost` |

`pub use` at the module root keeps every path. Prove the seams with `restructure check --deep` before applying them.
