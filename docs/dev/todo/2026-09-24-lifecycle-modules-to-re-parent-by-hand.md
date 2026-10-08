# 2026-09-24 — `tddy-session-lifecycle` modules that sit under the wrong parent

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
("Consent list" items 5 and 7)

## Why deferred

**The engine gap is closed; what remains has other reasons.** `tddy-tools restructure` now has
`reparent_module`, which moves a module's file and directory under another parent of the same crate, and
`move_item`, which moves items between modules of one crate. The modules of plan `11` whose destination is
a parent inside the crate were moved with them (`#carve` 16 follow-up). Three modules remain, because their
destination is no longer a module of this crate, and the two regroupings below were never planned as moves.

The wiring split ([#526](https://github.com/uppin/tddy-coder/pull/526)) moved topics out of the crate by
directory. A module left under the wrong parent either travels with the wrong topic or has to be picked out
by hand there; for the three below the answer is to move them straight to their receivers.

## Plan `11`'s modules that remain

Plan `11` moved nine misplaced clusters into modules of their own (`02b3f7a8`), each as a child of
the file it was found in. Six of the nine now sit under their topic's parent; three do not:

| Module (under `src/connection_service/`) | Holds | Belongs to |
|---|---|---|
| `svc_resolve_listed_worktree/session_dir_lookup.rs` | `session_dir_for` | the session catalog (left the crate in #526: `tddy-session-activity`) |
| `svc_host_builders/rpc_activity.rs` | `record_rpc_activity` | routing (`relay_idle`, in `tddy-daemon-kernel`) |
| `svc_host_builders/first_admission_token.rs` | `mint_first_admission_token` | room admission (`session_admission_service`, in `tddy-daemon-livekit`) |

Each destination is a receiver crate, so none is a `reparent_module`: they go to their receivers in `#carve`
17, with `move_module_to_crate`. `svc_host_builders.rs` itself now sits beside the struct it builds.

## `cli_spawn/`: the two non-sandboxed CLI spawns

The plan was one `src/cli_spawn/` directory: `claude.rs`, `cursor.rs`, and the Cursor spawn's
`chat.rs` and `resume.rs`. What the engine produced:

```text
src/connection_service/claude_cli_spawn.rs                        spawn_claude_cli_session_inner (plan 01)
src/connection_service/claude_cli_spawn/claude_cli_spawn_steps.rs its steps (plan 17)
src/cursor_cli_spawn.rs                                           spawn_cursor_cli_session_inner
src/cursor_cli_spawn/chat.rs                                      hooks, parse_created_chat_id, mint_cursor_chat_id (plan 03)
src/cursor_cli_spawn/resume.rs                                    resume_cursor_cli_session (plan 03)
```

The target:

```text
src/cli_spawn.rs                   mod claude; mod cursor;
src/cli_spawn/claude.rs            ← connection_service/claude_cli_spawn.rs
src/cli_spawn/claude/steps.rs      ← connection_service/claude_cli_spawn/claude_cli_spawn_steps.rs
src/cli_spawn/cursor.rs            ← cursor_cli_spawn.rs
src/cli_spawn/cursor/{chat,resume}.rs
```

Two constraints the move must keep:

- **`cursor_cli_spawn` is a `pub mod` at the crate root** (`lib.rs:69`), so its path is public
  surface. It stays as a `pub use crate::cli_spawn::cursor as cursor_cli_spawn;` (or the
  equivalent), or the move changes the public surface.
- `claude_cli_spawn` is glob re-exported into `connection_service` (`pub(crate) use
  claude_cli_spawn::*;`), and its three callers name it as `super::…`. Keep a `pub(crate) use` or
  repoint them.

## `ManagedWorkflow`

`ManagedWorkflow` (`src/session_toolcall.rs:57`) is the managed-workflow controller every agent
launch holds. It sits in the host core's toolcall module, but belongs with agent launch
(`connection_service/managed_launch.rs`). It is named through `tddy_daemon_sandbox`'s type-erased
`SessionScopedResource` (`session_toolcall.rs:76`), so the move is a path change only.

## What would close it

`ManagedWorkflow` is one `move_item`. The `cli_spawn/` regrouping needs its parent first: a `move_item` that
carries `name` creates an empty module, which `reparent_module` does not, and then one `reparent_module` per
module moves in. The public `tddy_session_lifecycle::…` paths are kept (`reexport: outside` leaves a facade
only for what another package reaches, and `cursor_cli_spawn` is a public module of the crate, so its path
must be kept by a facade or by `glob`). The three modules in the table above move with `#carve` 17. Then
`./test -p tddy-session-lifecycle` against the baseline by name, and every crate that depends on lifecycle
checked with `--all-targets`.

## Status

Partly resolved; narrowed at `#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)). The
`reparent_module` operation exists, the six in-crate rows are done, and two of the three receiver-bound
modules moved to their receivers (`session_dir_lookup.rs` to `tddy-session-agents`,
`first_admission_token.rs` to `tddy-daemon-livekit`). What remains:

- `svc_host_builders/rpc_activity.rs` (`record_rpc_activity`) is still a child of the wiring builders in
  `tddy-session-lifecycle`; its destination is routing (`relay_idle`, in `tddy-daemon-kernel`).
- The `cli_spawn/` regrouping. The five modules are flat files of `tddy-agent-launch` now
  (`claude_cli_spawn.rs`, `claude_cli_spawn_steps.rs`, `cursor_cli_spawn.rs`, `chat.rs`, `resume.rs`), and
  `cursor_cli_spawn` is a public module whose path the facade keeps, so the regrouping is a
  `reparent_module` inside that crate, with `pub use … as cursor_cli_spawn`.
- `ManagedWorkflow`, which sits in `tddy-cli-sessions`' `session_toolcall.rs` and belongs with
  `tddy-agent-launch`'s `managed_launch.rs`; `tddy-cli-sessions` may not depend on launch, so the type has to
  move into launch, and `tddy_daemon_sandbox`'s type-erased `SessionScopedResource` names it by path only.

No plan has been written for these.
