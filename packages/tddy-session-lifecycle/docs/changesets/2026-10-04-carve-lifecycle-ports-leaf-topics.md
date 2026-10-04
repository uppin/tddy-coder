# 2026-10-04 — The leaf topics run over state values, not the host

**Type:** Refactor

`#carve` 16/21 ([#531](https://github.com/uppin/tddy-coder/pull/531), 16a), the first of the stack
that converts `tddy-session-lifecycle`'s `impl DaemonSessionHost` methods in place into functions over
per-topic state, ahead of moving each topic into its receiver. This node converts the four leaf
topics and cuts the cross-topic edges that need no callback port. The layout it leaves is in
[module-layout.md](../module-layout.md).

No behaviour change and no public-surface change: every public `tddy_session_lifecycle::…` path is
where it was, no consumer crate (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`,
`tddy-desktop`) and not `tddy-session-agents` was edited, no crate edge was added, and every public
host method a consumer or a lifecycle test calls remains, as a delegator in `handler_state.rs`.

## What each milestone did

| Milestone | Commit | Change |
|---|---|---|
| M0.2 (engine part) | `d7b834f8` | `advertise_daemon_url`, `local_daemon_hook_url`, `claude_hook_daemon_url` and `DEFAULT_WEB_PORT` grouped into `hooks_and_urls/daemon_urls.rs` |
| M0.3 | `d7b834f8` | `split_forward_deadline`, `session_dir_for` and `mint_first_admission_token` became free functions of the fields they read; the host methods delegate |
| M0.5 | `d7b834f8` | `svc_split_context_from_codebase_host.rs`'s host-constructing inline test moved to the `#[cfg(test)]` sibling `split_context_from_codebase_host_tests.rs` |
| M1, T11 demo VM | `27aa824c` | `DemoVmState` {`demo_vm_state`, `tddy_data_dir`, `user_resolver`, `rpc_activity`, `config`}; `DemoVmServiceImpl` holds it instead of `Arc<DaemonSessionHost>` |
| M2, T10 presenter | `6b3f427f` | `PresenterObserverDeps` {`tddy_data_dir`, `presenter_event_sink`, `session_notification_bus`}; `SessionNotificationPublishing` and `resolve_session_label` moved into `session_notification_publishing`, behind the unchanged `session_notifications` facade |
| M3, T7 and T8 | `4e1e99b1` | `mint_first_admission_token` over `config` and `session_admissions` (no state struct); `resolve_os_user` in its own module `os_user_resolution`; `AttachmentState<'a>` {`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`} for the two attachment files; the host-free leaves (`placement`, `remote_git_pack_execution`, `agent_list_mapping`, `daemon_urls`) name their defining crates |

Engine runs: `extract_module` (`check --deep`, `apply --dry-run`, `apply`; no refusal) built
`daemon_urls`, `session_notification_publishing` and `os_user_resolution`. Everything else is hand
conversion limited to re-pointing a field read (`self.x` to a parameter or `state.x`) or a host call,
with no re-typed logic, no reordered statement and no dropped comment (compared as a comment
multiset per commit; the only differences are re-pointed paths in doc links and one reflowed
paragraph).

## Before and after

Measured with the inline-test-block rule (a production line is a line outside every `#[cfg(test)]`
item; test-only files excluded), from the merge base with `master` (`6faef323`) to the branch tip.

| | Before | After |
|---|---:|---:|
| `src/` files with production code | 112 | 115 |
| Production lines | 20,628 | 20,775 |
| Functions over 150 lines | 6 | 6, none grown |

| File (`src/connection_service/` unless noted) | Before | After |
|---|---:|---:|
| `handler_state.rs` (builders and delegators) | 116 | 208 |
| `connection_service.rs` | 463 | 484 |
| `activity_hub.rs` | 19 | 36 |
| `svc_materialize_staged_attachment.rs` | 245 | 268 |
| `svc_resolve_tddy_tools_path/svc_host_builders/presenter_observer_spawn.rs` | 47 | 56 |
| `hooks_and_urls.rs` | 227 | 182 |
| `hooks_and_urls/daemon_urls.rs` | new | 49 |
| `svc_resolve_os_user.rs` | 193 | 178 |
| `svc_resolve_os_user/os_user_resolution.rs` | new | 23 |
| `svc_split_context_from_codebase_host.rs` | 471 | 450 |
| `session_notifications.rs` (`src/`) | 96 | 15 |
| `session_notifications/session_notification_publishing.rs` (`src/`) | new | 85 |
| `svc_resolve_tddy_tools_path/svc_host_builders/first_admission_token.rs` | 46 | 42 |
| `svc_resolve_listed_worktree/session_dir_lookup.rs` | 23 | 18 |
| `demo_vm_coordinate_handlers.rs`, `svc_demo_vm_ports.rs`, `session_attachment_materialization.rs`, `svc_spawn_split_agent.rs`, `presenter_observer_task.rs` | 241, 61, 125, 448, 120 | 242, 63, 123, 447, 119 |

The growth is the builders, the three state types and the delegators that keep the host's public
methods callable. The baseline for `./test -p tddy-session-lifecycle --no-fail-fast` (skipping
`sandboxed_bash_pty_action_streams_output`) is **575 passed, 22 failed, 1 ignored**, the same 22 by
name, after every milestone and on the tree rebased onto `6faef323`. The 22 are macOS-only: the
sandbox RPC bridge is never installed (16 tests in four suites) and `tddy-remote-git-repo` is not
built (6 `session_sync_livekit_acceptance` tests). Clippy with `-D warnings`, `cargo fmt --check` and
`cargo check --all-targets` on lifecycle, `tddy-daemon-rpc`, `tddy-telegram-control` and
`tddy-daemon` were clean (and on `tddy-session-agents` and `tddy-desktop` for the check). A first M3
run showed `signal_session_sends_sigint_to_pid` red, a timing flake that passes alone and on re-run.

## Premises the code contradicted

- M0.1 and M0.6 need to move items between modules of one crate, which no engine operation does
  (`extract_module` takes a contiguous run of one file).
- `DemoVmServiceImpl::new(Arc<DaemonSessionHost>)` is public and called by `tddy-daemon`'s runtime,
  so it stays a host-taking constructor in `svc_demo_vm_ports.rs`.
- `AttachmentState` is a borrowed state; attachments hand nothing to a task, so it needs no owned
  form.
- `PresenterObserverDeps` and `mint_first_admission_token` sit under wiring's `svc_host_builders`,
  reached from `handler_state.rs` through `pub(in crate::connection_service)`, because the re-parent
  was not done.
- The engine writes `mod x; pub use x::*;`, so `session_notification_publishing` is visible as
  `pub(crate)` only; `placement.rs` named `crate::livekit_peer_discovery` only in a doc link.
- `DemoVmState.config` is a clone where the old service read the host's field live. The host never
  assigns `config` after construction, so the two are equivalent; a reloadable config would need a
  shared handle.

## Deferred, and who carries it

Deferred until the `tddy-tools restructure` engine can do them. They were not done by hand, on the
developer's decision of 2026-10-04 (the 500-line file rule was deferred the same day; the 150-line
function budget was not waived).

| Item | Why | Carried by |
|---|---|---|
| M0.1: the T3 module `peer_session_answer` (`peer_has_no_such_session`, `split_pairing`, `resolve_worktree_root_for_session`, free `resolve_exec_tool_worktree`) | the four items live in four files; no same-crate multi-file move exists. TODO `2026-10-04-restructure-extract-module-cannot-gather-items-from-several-files` | [#532](https://github.com/uppin/tddy-coder/pull/532) (16b) re-plans around it |
| M0.6: the `seeded_clone_guard.rs` split (`SessionStdioEndpoint` to launch, `ExecToolRoute` beside `LocalExecTools`) | the same gap. Both types are still in that file | #532, with M0.1 |
| the T4 half of M0.2: `write_claude_hooks_settings` and `resolve_start_session_claude_binary` grouped with split | not contiguous in `hooks_and_urls.rs`; the same gap | [#533](https://github.com/uppin/tddy-coder/pull/533) (16c), or sooner once the engine can group non-contiguous items |
| M0.4 (D8): re-parenting the four mixed parent/child files | needs the developer's consent and a `reparent_module` operation. TODO `2026-09-24-lifecycle-modules-to-re-parent-by-hand`, still blocking the moves | [#536](https://github.com/uppin/tddy-coder/pull/536) (node 17) |

The engine friction met while planning is in TODO
`2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a`. TODO
`2026-09-24-lifecycle-files-over-the-400-line-target` (with a status section for the deferred
500-line rule) and `2026-09-24-lifecycle-functions-still-over-150-lines` stay in the backlog.

**Successor plans that assumed 16a delivered more.** #534 (16d) and #535 (16e) were written before
the deferrals and carry no note of them. They state, wrongly, that `jail_env_builders.rs` was
re-parented off the T3 file, that `SessionStdioEndpoint` moved to T1, that the four mixed files were
re-parented, and (16e) that `peer_session_answer` exists. #533 (16c) says
`resolve_worktree_root_for_session` "already went to T3 (16a)"; it did not. The facts are in the
table above.

## The stack, as cut

Five linear conversion nodes then one move node, decided 2026-09-26: 16a (this, #531), 16b agents
and `AgentHostCallbacks` ([#532](https://github.com/uppin/tddy-coder/pull/532)), 16c split
([#533](https://github.com/uppin/tddy-coder/pull/533)), 16d stack spawns and the jail and CLI-spawn
half of launch ([#534](https://github.com/uppin/tddy-coder/pull/534)), 16e the start and resume half
and the coordinate handlers ([#535](https://github.com/uppin/tddy-coder/pull/535)), then node 17 moving
every converted topic into its receiver ([#536](https://github.com/uppin/tddy-coder/pull/536)). Decided
here and carried by the successors: no `AdmissionState` struct (the admission-token function takes
its two fields); the leaf states need no callback trait; the inline host-constructing test stays in
lifecycle as a sibling file.

## Code issues

All eleven records in `packages/tddy-session-lifecycle/docs/code-issues/` were re-measured (function
length, fn line to closing brace) at the merge base and at the tip. None is closed and none changed:
the functions in 16a's files (`spawn_split_agent` 112, `start_split_claude_cli_session` 146,
`ensure_project_available_for_start` 99) are byte-identical; the others are in files 16a did not
touch. No record was deleted or edited, so no final measurement is recorded here. No record carries
`Claimed by:`, and no TODO entry was resolved by this change.
