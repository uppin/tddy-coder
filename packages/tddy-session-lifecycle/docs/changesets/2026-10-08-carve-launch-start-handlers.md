# 2026-10-08 — The session start, resume and coordinate handlers run over `LaunchSessions`, completing the in-place conversion

**Type:** Architecture

`#carve` 20/21 ([#535](https://github.com/uppin/tddy-coder/pull/535), plan label 16e, M7b + M8). The last host-bound
topic code of `tddy-session-lifecycle` is `impl LaunchSessions`, in place: `start_session_core` and its children, the
Claude CLI resume, project provisioning, and the session coordinate handlers (list, start, streamed start, connect,
resume, signal, delete, worktree snapshot). No topic module names `DaemonSessionHost`, a wiring module or a topic above
it. No behaviour change, no crate move, no consumer edit (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`,
`tddy-desktop`), no new crate edge. Fifth and last in-place conversion of the `#carve` stack, after the leaf topics
([`2026-10-04-carve-lifecycle-ports-leaf-topics`](./2026-10-04-carve-lifecycle-ports-leaf-topics.md)), the agent topic
([`2026-10-05-carve-agent-roster-handle`](./2026-10-05-carve-agent-roster-handle.md)), the split topic
([`2026-10-07-carve-split-sessions-handle`](./2026-10-07-carve-split-sessions-handle.md)) and the launch spawns
([`2026-10-07-carve-launch-sessions-handle`](../../../../docs/dev/changesets/2026-10-07-carve-launch-sessions-handle.md)).

Durable description: [`module-layout.md`](../module-layout.md#launch-sessions).

## State B

- `LaunchSessions` (`connection_service/launch_ports.rs`) holds the host's launch fields under the host's names:
  `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `claude_cli_manager`, `sandbox_manager`,
  `task_registry`, `session_stdio`, `agent_activity_hub`, plus `user_resolver`, `spawn_client`, `workspace_sandboxes`,
  `rpc_activity`, `session_agent_inference`, `session_rooms`, `hosted_agent_clones`, `session_admissions` and
  `worktree_observer` (the last read by `announce_worktree_ready`, which the plan had not named). It also holds the agent
  topic's `AgentRoster` handle, the split topic's `SplitSessions` handle, `PresenterObserverDeps` and
  `host: Arc<dyn LaunchHost>`. The plan's borrowed `LaunchState` and its owned handle are this one struct, as for the
  split and agent topics.
- The start, resume and coordinate handlers are `impl LaunchSessions` in the files they were in:
  `svc_start_session_core.rs` and its four children, `svc_resume_claude_cli_session.rs`,
  `session_coordinate_handlers.rs`, `svc_resume_session.rs` and `svc_signal_delete_session.rs`;
  `announce_worktree_ready` moved with them. The nine `SessionHandler` / `SessionService` entries
  (`svc_session_lifecycle_ports.rs`) and `SplitHost`'s `start_workspace_session` and `delete_session` call the handle.
  The streamed start's `self.clone()` hand-off to a task clones the handle, textually unchanged (Recipe B).
- New modules: `svc_ensure_project_available_for_start.rs` (`ensure_project_available_for_start`, `spawn_project_clone`,
  with their `ProjectClone`), `svc_index_workspace_worktree.rs` (`index_workspace_worktree`) and the leaf
  `daemon_hook_urls.rs` (`local_daemon_hook_url`, `advertise_daemon_url`, `claude_hook_daemon_url`,
  `DEFAULT_WEB_PORT`), a `pub(crate)` child of `connection_service` that both the split and the launch topics name.
  `hooks_and_urls.rs` keeps only the launch topic's free helpers; the `daemon_urls` module is gone.
- `session_entry_from_listing` is a free function in `session_coordinate_handlers.rs`: the list's entry mapping reads
  nothing from the handle.
- `DaemonSeedCloneClaimant` and its `SeededAgentClones` impl are in `agent_host_callbacks.rs`, beside the roster handle
  they hold. `svc_agent_roster_wiring.rs` keeps only the `RemoteSnapshotSource` impl.
- The dead host methods are removed (`attachment_state`, `prepare_session_attachments`, `seed_clone_claimant`,
  `maybe_spawn_presenter_observer`); `LaunchSessions` carries the live ones.
- No `impl DaemonSessionHost` block holds topic code. The wiring and builder files keep one (`connection_service.rs`,
  `svc_host_builders.rs` and `rpc_activity.rs`, `handler_state.rs`, `svc_agent_host_ports.rs` and
  `session_room_opening.rs`, `svc_session_lifecycle_ports.rs`, the other `svc_*_ports.rs` files,
  `svc_agent_roster_wiring.rs`, `terminal_bridge_impl.rs`, the three `*_delegators.rs`, `svc_resolve_os_user.rs`,
  `local_exec_tool_dispatch.rs`, `rpc_families.rs`, `svc_shut_down_children.rs`), plus two older exceptions that are not
  in the wiring set: `conversation_worktree_op.rs` and the builder half of `session_worktree_observer.rs`.
- `SplitHost` and `AgentHostCallbacks` are unchanged, and `LaunchHost` gains no method from this node (the four it has
  after `#keyring` 9/9 (#516) are #516's): each is defined once and implemented once, on the host, in wiring.

## Decisions

- **D1, Recipe B** (settled by #534): methods on a per-topic owned handle with the host's field names. Moving a method
  changes its `impl` header and its receiver paths only; parameter counts are unchanged, so
  `resume_claude_cli_session` keeps its signature.
- **The consented import respelling.** `retarget_impl` over `svc_resume_claude_cli_session.rs` was refused by
  `check --deep` (S6): the file already bound `LaunchSessions` as `use super::launch_ports::LaunchSessions;` and the
  engine read the `use` it would add as a clash (`E0255`). The refusal was reported and not worked around. The developer
  then consented (2026-10-08) to respelling that one `use` by hand as
  `use crate::connection_service::launch_ports::LaunchSessions;`, marked `TODO(restructure-retarget-impl-s6)`. After
  that `check --deep` gave no findings and the engine moved `resume_claude_cli_session` and
  `resume_session_at_session_coordinate`. The engine defect itself is open in the backlog entry
  `2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type`.
- **A second engine refusal, not worked around.** `reparent_module` of `hooks_and_urls/daemon_urls.rs` was refused
  (`daemon_urls` starts a path inside the `use` `pub use daemon_urls::*;`, which cannot be re-pointed). The items were
  moved with `move_item` into the new leaf `daemon_hook_urls.rs` instead, which removed the two T4 -> `hooks_and_urls`
  edges. The emptied module was deleted in a following commit of this node.
- **`move_item`, not `extract_module`,** for the two project modules: their pieces are not contiguous, and
  `ProjectClone`'s private fields need the struct and its function to land together.
- **Greps, not shape tests** for the acceptance checks (D11, the 2026-09-25 decision).
- **`connection_service.rs` growth deferred:** +2 `mod` lines; #536 also edits the file and removes the converted
  topics' declarations (consent 2026-10-07, carried).

## Before and after

| | Before (`origin/master`) | After |
|---|---|---|
| `tddy-session-lifecycle` suite (macOS, `--no-fail-fast`, `--test-threads=1`, binaries built, `--skip sandboxed_bash_pty_action_streams_output`) | 575 passed, 22 failed, 1 ignored | 575 passed, 22 failed, 1 ignored, the same 22 by name (baseline unchanged by name). One intermediate run read 574 / 23: `session_room_acceptance::a_commit_reaches_both_agent_participants_from_a_single_publish` failed on a LiveKit testkit container startup timeout and passes alone |
| `tddy-session-agents` | 75 passed | 75 passed |
| `start_session_core` | 381 lines | 386 |
| `resume_claude_cli_session` | 121 | 120 (signature `&self` + 6, identical) |
| `resume_session_at_session_coordinate` | 138 | 141 |
| `ensure_project_available_for_start` | 99 | 99 (moved to `svc_ensure_project_available_for_start.rs`) |
| `svc_start_session_core.rs` production lines | 488 | 494 |
| `session_coordinate_handlers.rs` production lines | 424 | 453 |
| `connection_service.rs` | 578 lines by `wc -l` (`origin/master`, after #516) | 581 `wc -l`; about 513 production lines |

The function growth is rustfmt re-wrapping calls the re-points lengthened; no control flow was added. Gates:
`cargo check --all-targets` clean on lifecycle, `tddy-session-agents`, `tddy-daemon-rpc`, `tddy-daemon` and
`tddy-telegram-control`; clippy `-D warnings` and `fmt --check` clean on lifecycle. `tddy-desktop` is CI's. The 16
sandboxed tests that fail on macOS with "sandbox RPC bridge not installed" and the six `session_sync_livekit_acceptance`
tests never run the sandboxed start, relaunch and delete paths here, so those rest on compiling, the edit rule and
Linux CI.

## Acceptance (A1-A9)

| # | Criterion | Result |
|---|---|---|
| A1 | no topic file names `DaemonSessionHost` | met (comments excluded) |
| A2 | no topic file names a wiring module | met, after `DaemonSeedCloneClaimant` moved into `agent_host_callbacks.rs` |
| A3 | no upward topic edge | met: a scripted rank check over 66 modules found 0 upward edges, after the two T4 -> `hooks_and_urls` edges were cut by `daemon_hook_urls.rs`. The ranking is this node's reading of the inventory, not an authoritative map |
| A4 | foundations named by their defining crate | met; `local_exec_tools.rs` was re-pointed by `repoint_facade_imports` (3 paths) |
| A5 | no host clone in a topic file | the grep matches exactly the three Recipe B lines in `svc_start_claude_cli_session.rs`, each cloning `LaunchSessions` |
| A6 | each callback trait defined once, implemented once, unchanged | met; no trait block changed against the parent |
| A7 | no consumer edit | met: the diff over the four consumer packages is empty |
| A8 | baseline | held, 575 / 22 / 1 by name |
| A9 | no `impl DaemonSessionHost` outside wiring | met for this node; the two older exceptions above remain |

`restructure verify --against <parent> --retarget DaemonSessionHost=LaunchSessions` cannot exit zero over hand
re-points: 363,816 statements before and 363,838 after, 14 re-pointed through a module qualifier, 3 visibility-normalised,
84 `cfg(test)` gate lines excused, **100 lost and 122 gained**. Every one maps to a listed hand edit or engine re-flow:
the `self.<m>()` -> `self.<field>.<m>()` and `self.launch_sessions().<m>()` -> `self.<m>()` re-points; the host-to-handle
re-points of the nine `SessionHandler` entries, `SplitHost::delete_session`, `start_session_core` and the stream task;
the `rpc_served_by_peer`, `classify_daemon_route`, `common_room_slot`, `record_rpc_activity`, `resolve_os_user`,
`resolve_exec_tool_worktree` and `ensure_session_room` re-points; the removed dead host methods and their doc lines; the
`DaemonSeedCloneClaimant` construction; the new fields, builder lines and docs; two clippy `ptr_arg` signature fixes after
the extract; and lines the engine re-flowed. No statement is unmapped.

What the engine did: `move_item` (the two project modules, `DaemonSeedCloneClaimant` and its impl, the URL helpers),
`retarget_impl` (9 impl blocks), `extract_method` (`session_entry_from_listing`) and `repoint_facade_imports` (8 runs).
By hand: the new fields and builder lines, the host re-points and wiring callers, the dead-method removals, the one
`use` respelling, the two clippy fixes and one restored comment.

## Code issues

Re-measured by function line to closing brace, `origin/master` against this head. None is clean, so none is deleted;
each record carries its row.

| Record | Before | After | Verdict |
|---|---:|---:|---|
| `complexity-svc-start-session-core-start-session-core` | 381 | 386 | regressed, kept open |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session` | 121 | 120 | unchanged; no parameter added under Recipe B |
| `complexity-svc-resume-session-resume-session-at-session-coordinate` | 138 | 141 | rustfmt wraps, kept open |
| `complexity-svc-resolve-listed-worktree-ensure-project-available-for-start` | 99 | 99 | unchanged; Location now `svc_ensure_project_available_for_start.rs` |
| `oversized-file-connection-service` | 578 `wc -l` (`origin/master`, after #516) | 581 (about 513 production) | regressed by three `mod` lines, deferred |

## Backlog

- **Resolved:** `2026-09-24-lifecycle-session-entry-from-listing-not-started` (`ListSessions`' entry mapping is still
  inline: `session_entry_from_listing` was not started): the lift is a free function in `session_coordinate_handlers.rs`.
  Deleted at this wrap. Moving it into a `session_list_entries.rs` module, which the entry sketched, stays with row 12 of
  `2026-09-24-lifecycle-files-over-the-400-line-target`; that entry now names the remainder itself.
- **Kept, touched and not fixed:** `2026-09-24-lifecycle-functions-still-over-150-lines`,
  `2026-09-24-lifecycle-files-over-the-400-line-target`,
  `2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type` (the engine defect; its source link
  now points here).

## Open items for the move node

- `session_coordinate_handlers.rs` (453) and `svc_start_session_core.rs` (494) move with the launch topic; none may reach
  500 before then. `connection_service.rs` (about 513) is re-measured after the stack lands; if it is at or under 500,
  its record is deleted.
- Move the topics with the engine only; a refusal means stop and ask. Edges that leave with the launch topic: the free
  files' `hooks_and_urls` calls, and `daemon_hook_urls.rs`, which the split and launch topics both name.
- `restructure verify` cannot exit zero over the hand re-points; account for it as above.
