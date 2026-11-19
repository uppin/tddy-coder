# Initial discovery — session liveness from live agent tabs

**Changeset:** [2026-11-19-session-liveness-from-agent-tabs.md](2026-11-19-session-liveness-from-agent-tabs.md)

## Combined Conclusions

- **The bug.** `list_sessions_in_dir` (`packages/tddy-session-activity/src/session_reader.rs:90`)
  computes `is_active = metadata.pid.map(is_pid_alive)` — `kill(pid, 0)` on the pid recorded in
  `.session.yaml`. Nothing clears `pid` when the agent exits (no production writer sets
  `pid = None`), so a recycled pid makes a dead session read active again.
- **The daemon already knows which agents are live.** `CliSessionManager`'s terminal index
  (`session_id → terminal_id → TerminalEntry`) only resolves a terminal while its PTY is registered:
  `PtyRuntime` drops the `PtyRegistry` control the moment the child exits
  (`packages/tddy-pty/src/runtime.rs:232-235`), `resolve_pty_handle` needs it
  (`cli_session_manager/pty_spawn.rs:367-398`), and `spawn_terminal_cleanup` deregisters the entry
  after the exit hook (`pty_spawn.rs:279-332`). Main and tabs alike. `PtyHandle.kind` tells agent
  terminals (`claude-cli`, `muse-cli`, …) from `"bash"` and `"tmux"`.
- **Session kinds and their liveness source:**

  | Kind | What the daemon holds | Today's `pid` |
  |---|---|---|
  | AgentCli PTY main (incl. project-home, orchestrator, split agent half) | `CliSessionManager` terminal `main` | the PTY child |
  | AgentCli `run_in_tmux` | `main` = a tmux *client* PTY; while detached, a `DetachWatch` (`tmux_reattach.rs`) | the pane pid |
  | AgentCli on a target (ssh/Coder) | `main` = the local transport client PTY; it exits when the remote agent ends | the local client |
  | Sandboxed CLI harness | `SandboxSessionManager` entry holding the runner's `Child` (via `SandboxHandle`) — **not removed on exit** | the runner |
  | `tool` (tddy-coder) | **nothing** — the child is held by the supervisor / spawn worker / a detached `wait()` thread; the daemon gets `SpawnResult.pid` only | written by the child itself |
  | `workspace` | no agent | `None` |
  | Agent / bash / tmux tabs | `CliSessionManager` terminals with fresh ids | — |

- **After a restart** the index is empty: a non-tmux main died with the old daemon (`kill_all`); a
  tmux-run agent survives but nothing re-attaches it until a stream or resume calls
  `reattach_recorded_main` (lazy).
- **Pid signalling hazards** (the same recycled pid, but *signalled*, not just probed):
  `session_deletion::delete_session_directory` → `terminate_session_process(metadata.pid)`;
  `signal_session_dir` (SignalSession + project-home TerminateSession) → `kill(metadata.pid, sig)`;
  `project_home_idle::stop_if_idle` waits on `process_alive(metadata.pid)`;
  `SandboxSessionState::stop` falls back to `terminate_sandbox_process(self.pid)` once its handle
  was already taken (i.e. already killed and reaped).
- **Delete leaves tabs running**: `forget_session_state` never calls `cli_manager.stop_session`.
- **Existing presence seam**: `OrchestratorPresence` / `ServedTerminalPresence` and
  `svc_ensure_project_home::is_served` already answer "main served", citing the same TODO.

## Exploration 1 — liveness sources and listing callers (Explore agent)

Patterns: `list_sessions_in_dir|is_pid_alive|fn pid_is_alive|fn pid_alive|fn process_alive|kill\(.*, 0\)|\.is_active\b`
over `packages/**/*.rs`; then reads of the files below.

### `list_sessions_in_dir` callers and `is_active` readers

- `tddy-agent-launch`: `session_coordinate_handlers.rs:74,390` (ListSessions → web drawer),
  `svc_ensure_project_home.rs:281,299` (`is_running = main.is_active || is_served`),
  `project_home_idle.rs:81-92,168-185`, `svc_pr_status_for_caller.rs:288-330`
  (`find_session_owning_branch(&DaemonSessionListing)`). All `impl LaunchSessions`, which holds
  `cli_manager` and `sandbox_manager` (`launch_ports.rs:85-92`).
- `tddy-daemon-rpc`: `exec_tool/project_home/listing.rs:23` (rows → `TargetRecord.is_active` →
  `turn.rs`, `stack_git_tools.rs`, `git_work.rs`, `tddy-worktree-service::session_target` →
  `session_git_ops.rs:152`, `tddy-tool-engine::project_home_lifecycle_tools`);
  `project/project_sessions.rs:14-20` (soft-delete refusal, `lifecycle_listing.rs`);
  `pr_stack/ports.rs:182,758` (`DaemonSessionListing`); `exec_tool/watch_wake.rs:260` (lists, does
  not read `is_active`). `ProjectRpcHandler::from_host` / `PrStackRpcHandler::from_host` take
  `&DaemonSessionHost` but copy no manager.
- `tddy-worktree-service`: `branch_owner.rs:22-90` via the `SessionListing` port.
- `tddy-session-activity`: `remote_git_pack_execution.rs:47,71` (free fns behind resolver closures).
- `tddy-telegram-control`: `chaining_and_listing.rs:322,358`, `callbacks.rs:27-40`; the harness holds
  no manager, its optional `workflow_spawn` does.
- Tests that fake "active" with `std::process::id()`: `session_branch_conflict_acceptance.rs:143-170`,
  `tddy-telegram-control/tests/telegram_branch_conflict_acceptance.rs:174`.

### Other pid probes (audit)

| Probe | Use | Verdict |
|---|---|---|
| `session_reader::is_pid_alive` | listing + delete log | replaced (listing) / removed (delete) |
| `session_deletion::terminate_session_process(metadata.pid)` | delete | signals what the daemon holds instead; `tool` keeps the yaml pid until node 2 |
| `svc_signal_delete_session::signal_session_dir` | SignalSession, TerminateSession | signals the held main's pid; `tool` keeps the yaml pid until node 2 |
| `project_home_idle::process_alive` | reaper exit wait | waits on the session leaving the live set |
| `SandboxSessionState::stop` fallback `terminate_sandbox_process(self.pid)` | second stop | removed — the handle is the only holder |
| `cli_session_manager/terminals.rs` `kill_all` probe | pids of tasks the daemon itself just cancelled | kept — the daemon holds these processes |
| `sandbox_session.rs::terminate_sandbox_process::pid_alive` | wait after SIGTERM of a held runner | kept |
| `host_session_socket.rs:302`, `agent_tool_socket.rs:86`, `session_pointer.rs:86`, `contained_shell.rs:170`, `host_tooling.rs:525` | socket-owner / process-group checks, not session liveness | out of scope |
| tests (`*_acceptance.rs`, `sandbox_fixtures.rs`) | checks of processes the test itself spawned | out of scope |

### tmux and target sessions

- tmux: `tmux_session_alive` = `tmux has-session` (`tddy-cli-sessions/src/tmux.rs:329-331`);
  `DetachWatch` polls every 10 s; re-attach via `ensure_main` → `TmuxMainReattach` →
  `reattach_recorded_main` (`tddy-agent-launch/src/claude_cli_tmux.rs:80-140`).
- target: `remote_agent_launch.rs:246-262` (`spawn_rendered_client_main`), remote pid recorded in
  the target-side session pointer (`cli_session_pointer.rs:189-260`, #955).

### Test harnesses

`tests/agent_terminal_tabs_acceptance.rs` (stub main via `start_with_options`, tabs via
`terminal_session_service()`), `tests/claude_cli_tmux_reattach_acceptance.rs` (`PrivateTmux`,
`shut_down_children` + `restarted()`), `tests/ensure_project_home_world/mod.rs`
(`a_running_main`, `stop`), `connection_service/list_sessions_unit_tests.rs`.

## Exploration 2 — crate placement

`tddy-session-activity` depends on `tddy-daemon-sandbox`; `tddy-cli-sessions` depends on both; all
depend on `tddy-daemon-kernel`. A shared `LiveSessions` type therefore belongs in
`tddy-daemon-kernel`. `SandboxHandle::new(child, …)` is public, so a sandbox state is constructible
in a unit test over a plain `Child`.
