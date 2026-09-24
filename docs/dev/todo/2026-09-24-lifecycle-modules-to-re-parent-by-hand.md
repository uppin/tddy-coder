# 2026-09-24 — `tddy-session-lifecycle` modules that sit under the wrong parent, and need a hand `git mv`

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
("Consent list" items 5 and 7)

## Why deferred

**Needs developer consent, because of an engine gap.** `tddy-tools restructure`'s `extract_module`
writes the new module as a child of the file it cuts from. It cannot put the code under a different
parent, and `move_module_to_crate` moves between crates, not within one. So every move below is a
`git mv`, plus `mod`/`use` edits, by hand. The developer's rule for the destructure (2026-09-24) was
that no refusal is worked around with a hand move, so each waits for consent. The destructure
already cut the code out, so each is now a whole-file move.

The next node, the wiring split ([#526](https://github.com/uppin/tddy-coder/pull/526)), moves
topics out of the crate by directory. A module left under the wrong parent either travels with the
wrong topic or has to be picked out by hand there.

## Plan `11`'s modules: cut out of the wrong file, still under it

Plan `11` moved nine misplaced clusters into modules of their own (`02b3f7a8`), each as a child of
the file it was found in:

| Module (under `src/connection_service/`) | Holds | Belongs to |
|---|---|---|
| `svc_materialize_staged_attachment/split_claude_cli_start.rs` | `start_split_claude_cli_session` | split sessions (`split_session/`, `split_start.rs`) |
| `svc_resolve_os_user/session_attachment_materialization.rs` | `prepare_session_attachments`, `materialize_session_attachments` | attachments (`attachment_progress.rs`, `svc_materialize_staged_attachment.rs`) |
| `svc_resolve_os_user/local_exec_tool_dispatch.rs` | `run_exec_tool_locally` | exec tools (`local_exec_tools.rs`) |
| `svc_resolve_listed_worktree/session_dir_lookup.rs` | `session_dir_for` | the session catalog |
| `svc_resolve_listed_worktree/session_room_opening.rs` | `ensure_session_room` | rooms (`svc_ensure_session_room_for_agents.rs`) |
| `svc_turn_end_reporter/jail_env_builders.rs` | `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env` | the sandboxed launch |
| `svc_resolve_tddy_tools_path/svc_host_builders/presenter_observer_spawn.rs` | `maybe_spawn_presenter_observer` | activity (`svc_activity_ports.rs`, `presenter_observer_task.rs`) |
| `svc_resolve_tddy_tools_path/svc_host_builders/rpc_activity.rs` | `record_rpc_activity` | routing (`relay_idle.rs`) |
| `svc_resolve_tddy_tools_path/svc_host_builders/first_admission_token.rs` | `mint_first_admission_token` | room admission (`session_admission_service.rs`) |

`svc_host_builders.rs` itself sits under `svc_resolve_tddy_tools_path/` for the same reason (plan
`07`): the host constructor and its `with_*` builders were cut from the file that held them, and
that file is now 51 lines. `svc_host_builders.rs` belongs beside `connection_service.rs`'s struct.

An example of the hand move, for one row:

```text
# before
src/connection_service/svc_resolve_os_user.rs                    mod local_exec_tool_dispatch;
src/connection_service/svc_resolve_os_user/local_exec_tool_dispatch.rs

# after
git mv src/connection_service/svc_resolve_os_user/local_exec_tool_dispatch.rs \
       src/connection_service/local_exec_tools/local_exec_tool_dispatch.rs
src/connection_service/local_exec_tools.rs                       mod local_exec_tool_dispatch;
# plus the `use super::…` paths inside the moved file rebased one level
```

**Not contiguous, so left where it is:** `run_exec_tool_locally` moved, but its exec-tool siblings
in `svc_resolve_os_user.rs` did not. `resolve_exec_tool_worktree` (lines 56 and 214) and
`authorize_exec_tool_caller` (183) sit between the OS-user and peer-routing functions, so no one
range held them. They go with this row's move.

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

The developer's consent for the moves above, done as one reviewed hand commit (a `git mv` per
row, the `mod` lines, the rebased `use` paths), with the public `tddy_session_lifecycle::…` paths
kept. Then `./test -p tddy-session-lifecycle` against the destructure's baseline (61 targets,
622 / 22 / 1), and every crate that depends on lifecycle checked with `--all-targets`. Or an engine
operation that moves a module under a different parent within one crate; none exists.
