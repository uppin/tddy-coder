# 2026-09-25 — `check` does not see a moved module's **body** path to a module staying behind

**Category:** Future enhancement (engine defect: `check --deep` passes a move `apply` cannot build)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), move T5b
(`workspace_session` → `tddy-session-activity`). Found by reading the code before any apply, and
confirmed by a diagnostic `check --deep` that reported `no findings`. Nothing was applied.

## What happened

`workspace_session.rs` imports nothing from lifecycle's host in its header. It reaches the host's
`service_util` through inline-qualified paths in function bodies:

```rust
// packages/tddy-session-lifecycle/src/workspace_session.rs
let (_, project) =
    crate::connection_service::find_registered_project(tddy_data_dir, os_user, project_id)?;
let repo_root = crate::connection_service::project_repo_root(&project)?;
…
    ..crate::connection_service::starting_session_metadata(session_id, project_id, "workspace")
```

`connection_service` is `DaemonSessionHost`'s module, and those three fns are `pub(crate)`. A
diagnostic plan moving `session_list_enrichment`, `session_notifications` and `workspace_session`
to `tddy-session-activity` returned:

```text
op 2 survey: packages/tddy-session-lifecycle/src/workspace_session.rs -> tddy-session-activity (tddy_session_activity), 5 item(s) reached from outside, 12 caller(s)
…
no findings
```

After the move, `crate::connection_service` names nothing in the destination (`E0433`). Re-pointed
at lifecycle, it is both a cycle and a private path (`E0603`).

## Why

The stays-behind finding reads the moved file's **header**: its `use` declarations. The plan schema
says so ("the finding is about a *moved* module whose header still names a module staying behind"),
and the [known limitations](../../ft/coder/rust-code-restructuring.md#known-limitations) describe
the header pass as mechanical. A `crate::x::y(…)` in a body is an edge just as real as
`use crate::x::y;`, and here it is the only one. The same file's `session_deletion` sibling was
caught, because its edge (`use crate::session_reader::is_pid_alive;`) is a header line.

## What would fix it

Where the finding collects the paths a moved file names, it could collect every path that opens
with `crate::`, `super::` or the origin's extern name, bodies included. `move_cluster_to_crate` is
not the remedy here: `connection_service` is the host, not a sibling that can come along. The
finding's remedy text should say so when the module staying behind is one the plan could never
move.
