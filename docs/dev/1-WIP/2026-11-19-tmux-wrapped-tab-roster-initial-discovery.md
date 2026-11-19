# Initial discovery — tmux-wrapped-tab-roster (`#session-liveness`)

**Changeset:** [2026-11-19-tmux-wrapped-tab-roster.md](2026-11-19-tmux-wrapped-tab-roster.md)


Node 1's discovery lives on its own branch
(`docs/dev/1-WIP/2026-11-19-session-liveness-from-agent-tabs-initial-discovery.md`, PR #958).

## Combined Conclusions

- **No terminal roster is persisted anywhere.** `SessionMetadata`
  (`packages/tddy-changeset/src/session_metadata.rs:12-174`, `deny_unknown_fields`) records only the
  main's `tmux_session` (:149), the main's conversations (`agent_conversations`, :163) and the
  specialized-agents roster (`agents`, unrelated to terminals). It does not record the session's OS
  user. Nothing locks it: `write_session_metadata` (:356) is atomic via rename, but about 27 writers
  do an unlocked read-modify-write, including the frequent hook-driven `update_activity_status`
  (:373). **Decision:** the roster goes in its own file in the session dir (`tabs.yaml`), written
  only by the daemon's terminal roster under one lock.
- **Only `main` can run wrapped in tmux.** `attach_main_to_tmux`
  (`cli_session_manager/tmux_main.rs:21-43`) hard-codes `MAIN_TERMINAL_ID`. The tmux name is
  per-session (`agent_tmux_session_name` = `tddy-<session_id>`, `tmux.rs:215`). `DetachWatch`, its
  `Watchers` map and the exit classification are keyed by session and main only
  (`tmux_reattach.rs:26-29, 66-148`; `pty_spawn.rs:279-330`). `spawn_tool` already takes
  `terminal_id` and `tmux_session` as parameters (`pty_spawn.rs:75-109`).
- **Agent tabs** (`CliManagerTerminalRoster::start`,
  `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs:300-333`) mint a
  `uuid::Uuid::now_v7()` conversation id and render argv with `session_id` = `chat_id` = that id.
  They get no env, no os_user, no hooks, no `resume` flag and no exit hook, and nothing is persisted.
  Terminal ids are minted inside `start_terminal`, `start_agent_terminal` and `start_tmux_terminal`
  (`terminals.rs:32/59/89`); the proto has no requested id.
- **Proto** `packages/tddy-terminal-rpc/proto/terminal_session.proto`:
  `StartTerminalSessionRequest{session_token, session_id, control_token, kind, tmux_session}`,
  `TerminalSessionInfo{terminal_id, kind, pid, tmux_session}`. `TerminalKind::from_request`
  (`tddy-terminal-rpc/src/service/ports.rs:97-131`) maps it to `Tmux` / `TmuxNew` / `Bash` / `Agent`.
- **Daemon startup** (`tddy-daemon/src/runtime.rs:318-337`, `mount_host.rs`) scans the sessions
  dir once (`sweep_what_the_last_run_left`, `runtime/last_run_sweep.rs:29`) and re-attaches nothing.
  Re-attach is lazy: `ensure_main` (`tddy-terminal-rpc/src/service.rs:103-119`) →
  `TmuxMainReattach` (single-flight, `svc_terminal_ports.rs:138-218`) →
  `reattach_recorded_main(manager, agents, tddy_data_dir, os_user, session_id) -> bool`
  (`tddy-agent-launch/src/claude_cli_tmux.rs:80-155`), which already fits an eager loop. All users
  share one sessions base (`sessions_base_for_user` returns the data dir); enumerate them as
  `LaunchSessions::session_bases()` does (`project_home_idle.rs:114-127`). `agent_tool_sockets` must
  be bound before a re-attach.
- **Tool sessions.** `spawn_tddy_coder` (`tool_session_spawn.rs:177-261`) returns
  `SpawnResult{…, pid, grpc_port}`, with no handle or exit future.
  - Direct spawn: a log-only `child.wait()` thread (`tddy-spawn/src/spawner.rs:~1218-1231`).
  - Forked worker: `WorkerResponse{SpawnOk, CloneOk, Error}`, events `{Started, CloneProgress}`, and
    **no exit message**; the child belongs to the worker process.
  - Supervisor: `SessionStatus(SessionRef{pid})` → `RUNNING | EXITED, exit_code`, poll only; the
    supervisor owns the child, so its answer is exact.
  - The pid is written by the child itself (`tddy-coder/src/run.rs:1526,1910,3918`).
    `HostSessionSockets::watch_until_stopped` (`host_session_socket.rs:128-153`) polls
    `process_exists(pid)`, which has the same pid-reuse hazard.
  - `TaskRegistry` can hold a non-PTY task (`status`, `status_watch`, `cancel`, `pid_slot`;
    `tddy-task/src/registry.rs:102-138`), but `resolve_pty_handle` and `list_terminals` require a PTY.
- **Resume** (`svc_resume_session.rs:34-230` → `opencode_cli_spawn/resume.rs:39-436`,
  `cli_harness_tmux.rs:111-199`) restores **main only**. Main's conversation choice lives in
  `cli_agent_steps/conversation.rs:18-241`. Per spec:

  | Spec | Can resume a given conversation id? |
  |---|---|
  | claude | yes — `--resume {session_id}` with `resume` set; a tab today renders `--session-id <fresh>` |
  | cursor | yes with a real minted chat id; a tab today passes a random uuid as `chat_id` (suspect) |
  | opencode, muse | no — `--continue` (the worktree's last, ambiguous across tabs) |
  | warp | needs a token captured by an exit hook; tabs have none |

  **Decision:** a tab that cannot be resumed reopens fresh under its recorded id and is marked
  not-resumable; claude and cursor tabs resume their own conversation.
- **Web.** `packages/tddy-session-web/src/useSessionTerminals.ts:66-192` lists terminals once per
  session and keeps `activeTerminalId` in `useState` only. Resume:
  `useSessionsDrawerPaneActions.ts:91-120` → `useSessionAttachment.ts:139-141` → `markResumed`. The
  "Add tab" menu is `NewTerminalMenu.tsx`.

## Exploration 1 — State A for nodes 2–4 (Explore agent)

Prompt: State A for (a) a per-session tab roster, (b) tmux-wrapped agent tabs, (c) daemon startup
re-attach, (d) tool-session process holders per spawn backend, (e) resume of main and tabs, (f) web
tab strip and Resume, (g) test harnesses per node. Findings are distilled into Combined Conclusions
above; the remaining detail:

- **Metadata writers without a lock:** `set_pending_elicitation` (:389); `persist_roster`
  (`tddy-session-agents/src/session_agent_roster.rs:336-356`, serialised only in-process);
  `record_rotation` (`tddy-session-activity/src/service/conversation_rotation.rs:89-113`);
  `record_the_resumed_session` (`resume.rs:418-436`); plus about 27 files calling
  `write_session_metadata`.
- **tmux start:** `run_in_tmux` is checked in `start_request_checks.rs:135-175` (refused with
  sandbox, sandboxed codebase, or a codebase daemon). It leads to `harness_launch.rs:213` and
  `start_or_resume_in_tmux` (:275-285). The primitives in `tmux.rs`: `start_agent_tmux_session`
  (:252, writes a 0600 pane-env file), `kill_tmux_session` (:311), `tmux_session_alive` (:329),
  `pane_pid` (:348), `tmux_attach_argv` (:48), `tmux_new_session_argv` (:76), `ensure_reachable`
  (:130), `list_tmux_sessions` (:155).
- **`reattach_recorded_main`** steps: validate the id → read metadata → require `tmux_session` →
  `agent_tmux_session_is_alive` → `recorded_cli_session_type` → `reattach_recorded_harness`
  (`cli_harness_tmux.rs:218-264`: Running → reattach; Exited with a hook → run the hook; Exited
  without one → kill; Gone → false) → `record_main_terminal`.
- **Background-loop pattern:** `RuntimeTasks::spawn` (`runtime/tasks.rs:261-391`),
  `project_home_reaper` (:303-322).
- **Supervisor proto:** `packages/tddy-supervisor/proto/supervisor.proto`. `SpawnSession →
  SpawnedProcess{pid}`; `SessionStatus` (:186-204); `StopSession`; only `SpawnStreamed` streams
  `exited` (:148-184). `watch_session_startup` (`supervisor_spawn.rs:178-215`) polls during the
  startup grace only.
- **Spawn worker:** `spawn_worker.rs:141-158` (requests and responses), `spawn_worker/wire.rs:36-43`
  (events), `client.rs:46`, `serve.rs:17-60`; `run_spawn` (:233) runs `spawn_as_user` in the worker.
- **Delete / signal for tool sessions:** `signal_session_dir` (`svc_signal_delete_session.rs:381-440`)
  and `delete_session_as` (:90-161). `forget_session_state` unregisters the host-session socket and
  signals nothing; the child is killed by `delete_session_directory` through the recorded pid.
- **Web resume liveness:** `resumedSessions.ts`, `useResumedSessionLiveness`
  (`SessionsDrawerScreen.tsx:227`); tabs render in `SessionTerminalTabs.tsx`, consumed by
  `SessionRuntime.tsx:117-130` and `DormantSessionRuntime.tsx:87-93`.
- **Harnesses:**
  - tmux-roster: `tests/support/mod.rs:36` (`PrivateTmux`), `claude_cli_tmux_reattach_acceptance.rs`,
    `cli_harness_tmux_acceptance.rs`, `tmux_tabs_acceptance.rs`, `tmux_new_session_acceptance.rs`,
    `agent_terminal_tabs_acceptance.rs`, `dormant_session_tabs_acceptance.rs`,
    `tddy-cli-sessions/tests/tmux_reattach.rs`.
  - tool-tab: `tool_session_host_socket.rs`, `supervisor_spawn_delegation.rs`, `delete_session.rs`,
    `signal_session.rs`, `tddy-spawn/tests/worker_concurrency_acceptance.rs`,
    `tddy-supervisor/tests/supervisor_session_lifetime.rs`.
  - resume-roster: `claude_cli_tmux_reattach_acceptance.rs:409,439,501`,
    `cli_harness_tmux_acceptance.rs:390,461`, `cursor_cli_resume_chat_acceptance.rs`,
    `agent_terminal_tabs_acceptance.rs:258`; web
    `cypress/component/SessionTerminalTabsAcceptance.cy.tsx`, `src/resumedSessions.test.ts`.

## Exploration 2 — deferred-work scan (Step 2b)

`grep -rl -iE 'tmux|resume|roster|tool session|spawn worker|supervisor' docs/dev/todo/` plus code
issues under the touched packages. Candidates are read per node and recorded in each node's
`## Prerequisites`.

## Exploration 3 — node-specific

- `spawn_tool(session, terminal_id, kind, worktree, model, argv, env, os_user, exit_hook, tmux_session)`
  (`pty_spawn.rs:75-103`) already takes a terminal id; `start_*` mint one only because their callers
  never had a recorded one.
- `reattach_recorded_main`'s signature `(manager, agents, tddy_data_dir, os_user, session_id) -> bool`
  is the per-session unit the startup task loops over. Per-terminal re-attach generalises its last
  two steps.
- The startup task belongs beside `project_home_reaper` in `runtime/tasks.rs`, and runs once after
  `mount_host` binds the agent-tool sockets.
