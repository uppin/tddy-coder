# 2026-10-07 — `retarget_impl` refuses a file that already imports its target type by a `super::` path

**Category:** Restructure engine defect (worked around once, with the developer's consent; the engine is not fixed)
**Source:** `#carve` 20/21, [`2026-10-08-carve-launch-start-handlers`](../../packages/tddy-session-lifecycle/docs/changesets/2026-10-08-carve-launch-start-handlers.md), M7b.3

## What happened

`retarget_impl` over the whole `impl DaemonSessionHost` block of
`packages/tddy-session-lifecycle/src/connection_service/svc_resume_claude_cli_session.rs`, to
`tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions`, is refused by
`restructure check --deep`:

```
5: this seam cannot be cut here: `LaunchSessions` is already bound in
   `packages/tddy-session-lifecycle/src/connection_service/svc_resume_claude_cli_session.rs`
   to something else: a `use` of `crate::connection_service::launch_ports::LaunchSessions`
   would clash (`E0255`)
```

The file already holds `use super::launch_ports::LaunchSessions;`, which is the **same item**. The
engine compares the two `use` spellings as text (S6), so it reads one binding of `LaunchSessions` to
`super::launch_ports::LaunchSessions` and the `use` it would add as a different one.

## Why it matters

`resume_claude_cli_session` is the T1 resume half. While it stays `impl DaemonSessionHost`:

- `resume_session_at_session_coordinate` (`svc_resume_session.rs`) cannot move to the launch handle,
  because the handle may not name the host (A1) and the callee is a host method;
- `grep -rln 'impl DaemonSessionHost'` still lists those two non-wiring files (A9).

## What would close it

Either of:

1. **Engine:** S6 resolves both `use` paths relative to the file's module before comparing, and treats
   two spellings of one item as already bound (no second `use`, no refusal).
2. **A one-token edit, with the developer's consent:** respell `use super::launch_ports::LaunchSessions;`
   as `use crate::connection_service::launch_ports::LaunchSessions;` in that file, then run the same
   `retarget_impl` (and the one for `svc_resume_session.rs`, then re-point its `SessionHandler` entry
   with `repoint_call`). It was **not** done in `#carve` 20/21: a refused engine operation is stopped on,
   not worked around.

Proven by `check --deep` on a warm index daemon; nothing was written for this operation.

## Status (2026-10-08)

Option 2 was taken for `#carve` 20/21 with the developer's consent, relayed by the coordinator: the import in `svc_resume_claude_cli_session.rs` is respelled by hand and carries `TODO(restructure-retarget-impl-s6)`. The engine defect (option 1) stays open.
