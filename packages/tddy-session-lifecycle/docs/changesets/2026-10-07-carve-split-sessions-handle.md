# 2026-10-07 — The split topic runs over an owned `SplitSessions` handle and a `SplitHost` port

**Type:** Architecture

Every split-session method of `DaemonSessionHost` (topic 4 in [module-layout.md](../module-layout.md)) is
`impl SplitSessions`, an owned handle over the host's split fields, in place. The topic's modules, with
`service_util`, `workspace_session` and the PTY runtime beside them, name neither `DaemonSessionHost` nor a wiring
module, and the topic no longer calls up into the launch topic, so it can move into its own crate as a plain
module move. No behaviour change, no crate move, no public-surface change: no consumer crate (`tddy-daemon-rpc`,
`tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`) was edited and no crate edge was added. This is the third
in-place conversion of the `#carve` stack, after the leaf topics
([`2026-10-04-carve-lifecycle-ports-leaf-topics`](./2026-10-04-carve-lifecycle-ports-leaf-topics.md)) and the agent topic
([`2026-10-05-carve-agent-roster-handle`](./2026-10-05-carve-agent-roster-handle.md)).

## State B

- `SplitSessions` (`connection_service/split_ports.rs`) holds the host's split fields under the host's names
  (`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `session_rooms`, `workspace_sandboxes`,
  `workspace_sandbox_provisioner`, `claude_cli_manager`, `session_tokens`), plus the agent topic's `AgentRoster`
  handle and `host: Arc<dyn SplitHost>`. It is `Clone`; the host builds it per call (`split_sessions()` in
  `handler_state.rs`). It lends `AttachmentState` and makes the `RemoteSnapshotSource` a room's poll loop
  measures a peer's checkout with.
- The split methods are `impl SplitSessions` in the files they were in: `svc_spawn_split_agent.rs`,
  `svc_spawn_split_agent/svc_paired_codebase_teardown.rs`, `split_start/split_claude_cli_start.rs`,
  `svc_split_context_from_codebase_host.rs`, `svc_start_sandboxed_codebase_session.rs`,
  `svc_resolve_tddy_tools_path.rs`. Moving a method changed its `impl` header and its receiver paths.
- `trait SplitHost: AgentHostCallbacks` has five methods: `start_workspace_session`, `delete_session`,
  `session_files`, `session_agents`, `session_room_roster`. It is defined once, in `split_ports.rs`, and implemented
  once, on the host, in `svc_agent_host_ports.rs`. `start_workspace_session` and `delete_session` are the two
  upward re-entries into the launch topic, cut here.
- `AgentHostCallbacks` is unchanged since the agent topic's node except that `worktree_snapshot` lost its
  `#[allow(dead_code)]`: `join_split_livekit_room` calls it through the handle.
- Extracted into modules of their own: `resume_split_wiring` and `split_roster_from_codebase_host`
  (`svc_resume_claude_cli_session/svc_resume_split_wiring.rs`) and `provision_workspace_tool_sandbox`
  (`svc_ensure_session_room_for_agents/svc_provision_workspace_tool_sandbox.rs`).
- `attached_initial_prompt` moved down from the launch topic into `attached_initial_prompt.rs`, a free function over
  `AttachmentState`. `workspace_sandbox_spec` moved from `jail_relaunch.rs` to `workspace_session.rs`.
- One test still calls a split method on the host, so `svc_split_delegators.rs` holds a `#[cfg(test)]` forward.
- The split topic's free files, `service_util`, `workspace_session` and the PTY runtime (`cli_session_manager` and
  its children) name foundations by their defining crate, not through a lifecycle facade.

## Decisions

- **D1, owned handle,** as for the agent topic: methods on a per-topic handle with the host's field names.
  There is no borrowed `SplitState`: no free function in the topic takes one, so it would be dead code.
- **D3, five callbacks.** `session_files` and `session_agents` are the two wiring services the topic reads context
  and a roster through. `session_room_roster` is a `SplitHost` method because the agent topic's trait does not
  carry it. They return `Arc<dyn SessionFilesService<…>>` and `Arc<dyn SessionAgentService<…>>`, spelled once as
  the aliases `SplitSessionFiles` and `SplitSessionAgents`, because the wrapper types are wiring and both traits
  have associated stream types. `agent_tool_socket` is not a callback: it is a function of `tddy_data_dir`.
- **`attached_initial_prompt` moves down** rather than becoming a callback, so the topic does not call up.
- **D5:** `service_util` and `workspace_session` belong to the split topic: both are used by the launch topic above
  it, and `split_start` names `workspace_session::PairedAgentSession`.

## Before and after

| | Before | After |
|---|---|---|
| `tddy-session-lifecycle` suite (`--no-fail-fast`, `--test-threads=1`, macOS, with the binaries `./test` builds) | 575 passed, 22 failed, 1 ignored | 575 passed, 22 failed, 1 ignored (re-run on the final head, binaries built); the failing set is identical by name |
| `tddy-session-agents` | not re-measured on the parent | 75 passed (the node added no test) |
| `connection_service.rs` production lines | 497 | 503 (see Open items) |
| `start_split_claude_cli_session` | 146 lines | 149 |

The 22 failures are the macOS-only sandbox and LiveKit suites listed in [test-suites.md](../test-suites.md); Linux CI
runs them. Run without the binaries `./test` builds first, the same suite reads 563 passed and 34 failed: twelve
sandboxed-codebase tests die with exit code 127 because `tddy-sandbox-runner` is not on disk. Those twelve exercise
`start_sandboxed_codebase_session`, `delete_paired_codebase_session` and the resume wiring, and pass with the
binaries built, so they guard those paths on macOS. The sixteen sandboxed tests that fail with "sandbox RPC bridge
not installed" and the six `session_sync_livekit_acceptance` tests cannot run on macOS, so the start, relaunch and
delete paths they cover rest on compiling, the edit rule and Linux CI. `cargo check --all-targets` is clean on
lifecycle, `tddy-session-agents`, `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control`; clippy `-D warnings`
and `fmt --check` are clean on lifecycle. `tddy-desktop` is CI's.

## Acceptance

| # | Criterion | Result |
|---|---|---|
| A1 | no converted file names `DaemonSessionHost` | pass, except the launch topic's `resume_sandboxed_claude_cli_session` in `svc_split_context_from_codebase_host.rs` (its `use` and its `impl`) |
| A2 | no converted file names a wiring module | pass |
| A3 | no upward topic edge | pass by grep, after one cut the plan had not named: `provision_workspace_tool_sandbox` called `jail_relaunch::workspace_sandbox_spec`, now in `workspace_session` |
| A4 | foundations named by defining crate | pass; the engine resolved `PtyReady` and the terminal-size constants to `tddy_pty::runtime`, and `PtyRuntime` and `PtySpawnSpec` to `tddy_terminal_rpc::pty_runtime` |
| A5 | no host clone in a converted file | pass |
| A6 | one trait, one impl in wiring, five methods, `AgentHostCallbacks` unchanged, no `LaunchHost` | pass |
| A7 | no consumer edit, every facade resolves | pass |
| A8 | baseline held | pass (above) |

`restructure verify` cannot exit zero for an extract, so it was accounted for by hand: every one of the 32 lost
statements has its counterpart among the gained ones, and each lost comment is deliberate (the removed
`dead_code` TODO, and the comment for a now-unused `use`).

## What the engine could and could not do

The engine did: `retarget_impl` (8 operations), a bulk `repoint_call` for the four launch-topic callers, both
`extract_module` runs, a 27-file `repoint_facade_imports` and one `move_item`. It refused nothing. The `retarget_impl`
run's compile gate failed as documented, with 18 errors, all host calls and field reads. They were fixed by hand
under the edit rule: 22 re-points (a host call to a `SplitHost` callback, a field read to the handle, an
`Arc::new(self.clone())` hand-off to a callback), and the unused `use` lines the engine leaves when its gate fails.

## Code issues

Re-measured by function line to closing brace, `origin/master` against this head. None closed; none deleted.

| Record | Before | After | Action |
|---|---:|---:|---|
| `complexity-split-claude-cli-start-start-split-claude-cli-session` | 146 lines | 149 | +3 (re-points and a rustfmt wrap); row added. One line under the 150 budget |
| `complexity-svc-spawn-split-agent-spawn-split-agent` | 112 lines | 118 | +6, rustfmt wrapping the `attached_initial_prompt(..)` call; parameters unchanged; row added |
| `complexity-svc-paired-codebase-teardown-delete-paired-codebase-session` | 96 lines | 96 | unchanged; row added |
| `complexity-svc-start-session-core-start-session-core` | 374 lines | 377 | +3, three `.split_sessions()` re-points; row added |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session` | 119 lines | 120 | +1; row added; the split half moved out |
| `complexity-svc-resume-session-resume-session-at-session-coordinate` | 137 lines | 138 | +1; row added |
| `oversized-file-connection-service` | 497 | 503 | **new**, deferred (see below) |

No control flow was added to any function.

## Backlog

- **Created:** `oversized-file-connection-service` (a code-issues record) and a status section in
  `2026-09-24-lifecycle-files-over-the-400-line-target`. `connection_service.rs` crossed 500 production lines by three
  `mod` declarations. The developer consented to deferring the decomposition (2026-10-07) because the three
  dependents also edit the file and the stack's last node moves modules out of it.
- **Kept, touched and not fixed:** `2026-09-24-lifecycle-functions-still-over-150-lines`;
  `2026-09-24-lifecycle-modules-to-re-parent-by-hand` (still blocks the move node);
  `2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter`, which the
  conversion answered by hand and which stays open as an engine capability.
- **Cited by the plan and already gone from the backlog on `master`:** the entries on signature operations,
  `extract_module` gathering items from several files, the move-to-crate and check body-path gaps, and the two
  extract-method traps. Nothing here deleted or edited them.

## Wrong premises in the plan

`split_withdrawals_from_codebase_host` does not exist (the roster read is `split_roster_from_codebase_host`);
`split_claude_cli_start.rs` is under `split_start/`; `AgentHostCallbacks` has five methods, not three; the
`jail_relaunch` edge above; and `PeerRoutedSessionFiles` / `PeerRoutedSessionAgents` cannot be `SplitHost` return types.

## Open items for the next nodes

- The split topic moves into its own crate in the move node, with the engine only; a refusal there means stop and ask.
- `workspace_session.rs` still names three `service_util` items through
  `crate::connection_service::{find_registered_project, project_repo_root, starting_session_metadata}`, an intra-topic
  facade path (`service_util` is a private module of `connection_service`). The move node moves both.
- The T1 half of `svc_split_context_from_codebase_host.rs` (`resume_sandboxed_claude_cli_session`) and
  `index_workspace_worktree` stay host methods until the launch topic is converted (#534, #535).
- `connection_service.rs` is 503 production lines; re-measure after the stack lands.
