# 2026-11-24 — `svc_close_session/close_task.rs` is over the 500-production-line budget

**Category:** Deferred decomposition (developer consent, 2026-11-24)
**Source:** `/validate-changes` fixes on `feature/session-close-and-remove` (PR #1032)

`packages/tddy-session-lifecycle/src/connection_service/svc_close_session/close_task.rs` was 449
production lines after the restructuring commit (4d17b281e). The validation fixes (an unreadable tab
roster failing the check, nested-removal skipping, the planned presence of a worktree, clone listing
errors) and the W9 `extract_method` splits of `terminate_every_tab` took it to **601**, 101 over the
budget. The developer chose to defer the split on 2026-11-24.

## What would close it

A `move_impl_members` plan, already written and passing `tddy-tools restructure check --deep`: the
terminate phases of `CloseRun` — `terminate_every_tab`, `end_the_main`, `end_every_tab_beside_main`,
`end_every_conversation`, `check_that_no_tab_survived` (one contiguous run of members, about 140
lines) — moved into a new `close_task/terminate.rs` (`name: "terminate"`, `to: …::close_task`).
Regenerate its anchors with `restructure anchors … --items` against the tree of the day, apply, and
re-measure: the file should land near 460.
