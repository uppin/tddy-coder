# 2026-10-05 — The deferred lifecycle moves, applied through `move_item` and `reparent_module`

**Type:** Refactor

The four steps `#carve` 16a ([#531](https://github.com/uppin/tddy-coder/pull/531)) deferred because the engine
had no same-crate move, performed with the operations that were built for them
([`tddy-code-restructuring`](../../../tddy-code-restructuring/docs/changesets/2026-10-05-same-crate-moves.md)).
No behaviour change. Eleven plans, one commit each, each applied with `check --deep`, `apply --dry-run`,
`apply` (which gates the tree itself) and `verify --against HEAD`; no hand edit survives. The layout it
leaves is in [module-layout.md](../module-layout.md).

## What moved

| Plan | Operation | Result |
|---|---|---|
| 01 | `move_item` x2 | `write_claude_hooks_settings` and `resolve_start_session_claude_binary` out of `hooks_and_urls.rs` into `service_util.rs` |
| 02 | `move_item` with `name` | `connection_service::peer_session_answer` created with `peer_has_no_such_session` |
| 03 | `move_item` x3 | `split_pairing`, `resolve_worktree_root_for_session` and the free `resolve_exec_tool_worktree` gathered into it from `split_session.rs`, `workspace_session.rs` and `svc_resolve_os_user.rs` |
| 04 | `move_item` x2 | `SessionStdioEndpoint` out of `seeded_clone_guard.rs` into `svc_start_claude_cli_session.rs`, `ExecToolRoute` into `local_exec_tools.rs` |
| 05-11 | `reparent_module` | `split_claude_cli_start` under `split_start`; `session_attachment_materialization` under `svc_materialize_staged_attachment`; `local_exec_tool_dispatch` under `local_exec_tools`; `session_room_opening` under `svc_ensure_session_room_for_agents`; `jail_env_builders` under `svc_start_sandboxed_claude_cli_session`; `presenter_observer_spawn` under `presenter_observer_task`; `svc_host_builders` (with `rpc_activity` and `first_admission_token`) from under `svc_resolve_tddy_tools_path` to beside the struct in `connection_service` |

Every plan used `reexport: outside`, so the callers inside the crate were re-pointed and a facade was left only
for what another package reaches: `workspace_session::resolve_worktree_root_for_session` and
`connection_service::resolve_exec_tool_worktree` keep their paths through a `pub use`. `split_pairing` had no
caller outside the crate, so `split_session::split_pairing`, a `pub fn` of a `pub mod`, has no public path any
more; nothing in the workspace named it.

`rpc_activity`, `first_admission_token` and `session_dir_lookup` stay where they are, because their
destinations left the crate in #526; they move straight to their receivers in `#carve` 17, by
`move_module_to_crate`. No consumer crate (`tddy-daemon-rpc`, `tddy-telegram-control`, `tddy-daemon`,
`tddy-desktop`) and not `tddy-session-agents` was edited.

## Before and after

Production lines by the inline-test-block rule (a line outside every `#[cfg(test)]` item; test-only files
excluded), at the merge base with `master` and at the branch tip.

| | Before | After |
|---|---:|---:|
| Non-test `src/*.rs` files | 115 | 116 |
| Their production lines | 20,756 | 20,776 |
| Files over 500 production lines | 1 (`cursor_cli_spawn.rs`, 532) | 1 (the same, 532) |
| `connection_service.rs` | 484 | 488 |
| `svc_start_sandboxed_claude_cli_session.rs` | 494 | 495 |
| `service_util.rs` | 293 | 325 |
| `local_exec_tools.rs` | 388 | 405 |
| `hooks_and_urls.rs` | 182 | 151 |
| `seeded_clone_guard.rs` | 136 | 110 |
| `svc_resolve_os_user.rs` | 174 | 137 |
| `peer_session_answer.rs` | — | 71 |

The 500-line budget for the files near it is deferred by the developer and was not worked on. No function grew
by more than a rustfmt wrap: `delete_paired_codebase_session` is one line longer (95 to 96) because the
re-pointed `split_pairing` path no longer fits on the line it was on.

Gates, recorded while the moves were applied, scoped to `tddy-session-lifecycle` unless named:
`./test -p tddy-session-lifecycle` was 575 passed, 22 failed, 1 ignored before the first plan and the same
after the last, **the same 22 names** (they are the failures listed in
[the 16a entry](2026-10-04-carve-lifecycle-ports-leaf-topics.md)); `cargo fmt --check` and
`cargo clippy -p tddy-session-lifecycle --all-targets -- -D warnings` were clean; `cargo check
--all-targets` was clean for `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-telegram-control`,
`tddy-daemon`, `tddy-desktop` and `tddy-session-agents`; the comment lines of the 41 touched sources
compared as a multiset lost none and gained none; `restructure verify --against <the commit before the first
plan>` differed only by re-pointed qualifiers, each paired with its re-pointed form.

## Engine defects the moves found

Each stopped the moves and was fixed in `tddy-code-restructuring`, test first (see its entry): `outside`
counting a package's `tests/` as inside, `check --deep` refusing an item-anchored `move_item`,
`extract_module` being unable to gather a topic from several files (so `move_item` with `name` creates its
destination), a later move not widening the destination's `mod` declaration and copying whole group imports,
the destination's own import of the moving item counted as a clash, and a re-parented module's `super::Name`
respelled through its old parent's private import.

## Backlog and code issues

Narrowed, not deleted: `2026-09-24-lifecycle-modules-to-re-parent-by-hand` (what remains: `rpc_activity`,
`first_admission_token`, `session_dir_lookup`, the `cli_spawn/` regrouping and `ManagedWorkflow`). Resolved and
deleted: `2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files`. Open and
unchanged: `2026-09-24-lifecycle-files-over-the-400-line-target`,
`2026-09-24-lifecycle-functions-still-over-150-lines`,
`2026-09-24-lifecycle-topic-files-to-fold-into-existing-siblings` (now foldable with `reparent_module`).

Code issues: of the 11 records, none was closed. Six were touched and carry a 2026-10-05 row:
`start_split_claude_cli_start` (moved, 146 lines unchanged), `delete_paired_codebase_session` (95 to 96),
`ensure_project_available_for_start` (99, unchanged), `resume_claude_cli_session` (119, unchanged),
`spawn_split_agent` (112 by fn line to closing brace, unchanged) and `start_sandboxed_claude_cli_session`
(342, unchanged). The other five (`spawn_cursor_cli_session_inner`, `handle_rpc`, `resume_session_at_session_coordinate`,
`start_session_core` and `start_sandboxed_cursor_cli_session`) were not touched, and their functions measure
the same at the merge base and at the tip.
