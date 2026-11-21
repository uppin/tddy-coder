# `tddy-cli-sessions`: module layout

The daemon's CLI session PTY runtime: PTY-hosted `claude`, `cursor` and every other registered CLI harness's sessions (`opencode`, `muse`, `warp`) and their terminals. About
**3.0k production lines** in 17 modules (2026-11-03); no function over 100 lines. It depends on neither
`tddy-session-split`, `tddy-agent-launch` nor `tddy-session-agents`.

| Module | Holds |
|---|---|
| `cli_session_manager.rs` | `CliSessionManager`, the PTY session manager and the origin of `TaskRegistry`; the struct, `ControlLeaseInfo` and the per-session exposure record |
| `cli_session_manager/pty_handle.rs` | `PtyHandle`, `send_input` |
| `cli_session_manager/control_lease.rs`, `argv.rs`, `launch.rs`, `pty_spawn.rs`, `relaunch.rs`, `terminals.rs`, `exposed_terminals.rs` | `impl CliSessionManager` blocks: the control lease, the argv a spawn builds, launch, the PTY spawn, relaunch, the local terminals, and the exposure of a session's terminal through a `TerminalExposure` (`ensure_exposed_terminal`, single-flight per session: each recorded terminal carries its own gate, held across the join, while the registry's lock is taken only to read and to write the record, so a slow join to one session holds up neither another session's join nor any other registry access) |
| `cli_session_manager/pty_terminal_service.rs` | `pty_terminal_service(handle)`: serves `terminal.TerminalService` against a PTY handle, built from `tddy-rpc` types only, so any transport can host it. Every open starts with a snapshot frame taken under the capture lock (`tddy_terminal_rpc::session::snapshot_and_subscribe`), then live frames stamped with contiguous offsets; a `request_snapshot` after the opening message, or a lagged receiver, re-takes the snapshot at a new sequence point. No raw replay and no `trigger_redraw` — `PtyHandle::trigger_redraw` remains for the host's legacy bridge |
| `cli_session_manager/main_exit.rs` | The one line logged when a session's `main` ends: `MainExitWatch { pid, started }` is taken at spawn and `log_main_exit` writes `main exited: session=<id> pid=<pid> outcome=<exit code / signal / stopped by the daemon> ran=<s>s` at `info` (a failure or non-zero exit at `warn`, target `tddy_daemon::claude_cli_session`) |
| `cli_session_manager/agent_end.rs` | The announcement that a session's agent ended (`#activity-wake` 1/3): `AgentEnd { session_id, os_user, outcome, exit_code }` on the manager's broadcast channel (`AGENT_END_CAPACITY`), taken with `CliSessionManager::agent_ends()`. The two announcing sites: `spawn_terminal_cleanup` via `DetachWatch::main_ended` (a main classified `AgentEnded`; a PTY-run main with `outcome_of(status)` and its exit code, a tmux-run main — whose status is its client's, not its agent's — as `exit code unknown` with no code) and the detach watcher's `Ok(false)` arm (a detached agent whose tmux session is gone: `exit code unknown`, no code). A tab and a tmux client ending while its agent lives announce nothing |
| `tmux.rs` | the host's tmux, everything **run as the session's OS user** through the kernel's `run_output_as_user`: `tmux_binary` (`TDDY_TMUX_BINARY`, else `tmux`), `tmux_attach_argv` (`-T clipboard attach-session -t =<name>`), `tmux_session_name` / `tmux_new_session_argv` (the `tmux-new` tab), `agent_tab_tmux_session_name(session_id, terminal_id)` (`tddy-<session id>-<terminal id>` for a tmux-wrapped agent tab, separators replaced as for the main's), `list_tmux_sessions` + `parse_list_sessions` ("no server running" is `[]`), `ensure_reachable` (refuses a user this daemon cannot become); and the agent's tmux session — `agent_tmux_session_name` (`tddy-<full session id>`, `:`/`.` → `_`), `agent_new_session_args` / `start_agent_tmux_session` (`new-session -d -s -c -- <pane command>`; returns the pane pid; the pane's environment — the user's `HOME`/`PATH` under the session env — goes through `tmux/pane_env.rs`: a fresh `0600` file under `<session dir>/tmux-env/` (`0700`), both handed to the session's OS user when the daemon is another, written as shell-quoted `export K='v'` lines; the pane runs `/bin/sh -c '. "$1"; rm -f -- "$1"; shift; exec "$@"' sh <file> <argv>`, so no value is on any argv; a failed start removes the file), `kill_tmux_session` (an absent session or server is not an error), `tmux_session_alive`, `pane_pid` (`=<name>:`); with `remain_on_exit` the start sets `remain-on-exit` on the pane (`keep_pane_after_exit`; a failure ends the session). `tmux/pane.rs` (re-exported) is the kept pane after the agent exits: `remain_on_exit_args`, `capture_pane_args` / `capture_final_screen` (`capture-pane -p -S - -t =<name>:`), `PaneState` / `pane_state` (`#{pane_dead}`; tmux 3.6a answers an absent session with success and no output, read as `SessionGone`). Lives here, below `tddy-agent-launch` and `tddy-session-lifecycle`, so both use it |
| `cli_session_manager/tmux_main.rs` | `attach_main_to_tmux(session_id, worktree, os_user, kind, name)`: the session's **main** terminal as a tmux client of the agent's tmux session — `attach_terminal_to_tmux` for `MAIN_TERMINAL_ID` |
| `cli_session_manager/tmux_terminal.rs` | any terminal of a session as a tmux client (`#session-liveness` 2/4): `attach_terminal_to_tmux(session_id, terminal_id, kind, worktree, os_user, tmux_session)` — run as `os_user`, recorded with `tmux_session = Some(_)` — and `reattach_terminal(…)`, which stops the terminal's detach watcher and returns a terminal already attached to that tmux session as it is, so a second re-attach adds no second client |
| `cli_session_manager/recorded_terminals.rs` | `start_terminal_under_id(session_id, terminal_id, worktree, RecordedTerminal::{Shell { shell_path }, Agent { kind, argv }})`: a terminal started under the id its roster recorded, refused for `main` (restored by its own resume path) and for an id already live; `start_terminal` and `start_agent_terminal` mint an id and call it. `tab_rosters()` is the daemon's one roster writer |
| `tab_roster_store.rs` | `TabRosterStore`: the **single writer** of every session's `tabs.yaml` (`tddy_changeset::tab_roster`) — `record(session_dir, TabEntry)` replaces an entry of the same terminal id, `forget(session_dir, terminal_id)` removes one; each is a read-modify-write under one lock per session directory, and an unchanged roster is not rewritten. Held by `CliSessionManager`, so there is one per daemon |
| `cli_session_manager/tmux_exit_hook.rs` | the exit-hook source of a tmux-run main (`#tmux-agent` 3/3): `run_exit_hook_on_agent_exit(session_id, os_user, name, hook)` starts a watcher polling `pane_state` every `EXIT_HOOK_POLL_INTERVAL` (1 s); once the agent has exited it runs the hook on `capture_final_screen` as the session's pending exit (so `exit_hook_settled` waits on it), then kills the tmux session. One watcher per session (`ExitHookWatchers`); `kill_all` closes the registry and aborts the watchers still polling — never a hook already running. `exit_hook_capture(MainExit, final_screen)` reads the pane only on `AgentEnded` |
| `cli_session_manager/tmux_reattach.rs` | a tmux-run agent outliving its client (`#tmux-agent` 2/3): `classify_main_exit(tmux_session, probe) -> MainExit` (`ClientOnly` when the recorded tmux session is alive, `AgentEnded` otherwise; a PTY-run main never probes) and `classify_ended_main` (the probe on the blocking pool; one that does not finish is `ClientOnly`, so nothing is torn down on a guess); `reattach_main(session_id, os_user, name, worktree, kind)` — `reattach_terminal` for `main`; `has_managed_workflow`; and the **detach watcher** `DetachWatch` (per terminal: `for_terminal(terminal_id)`; the watcher registry is `session → terminal → watcher`, and `settling` holds `(session, terminal)` pairs, so a main and each tmux-wrapped tab are watched on their own — a tab's watcher drops no session state and announces no agent end when its tmux session goes): `main_ended` for an `AgentEnded` main drops the managed workflow and the exposure; for a `ClientOnly` main `hold_detached_agent` starts a watcher polling `tmux_session_alive` every `DETACH_WATCH_INTERVAL`, 10 s, `with_detach_watch_interval` for tests, which runs the same cleanup once the tmux session is gone. The watcher takes the session over **before** the client's terminal leaves the index, and `settling` / `settled` hold the session from the client's end until its agent has been classified, so a live-set read never finds a live agent held by neither. The watcher registry is closed by `kill_all`, so the clients a shutdown cancels start no watcher |
| `cli_session_manager/reattach_outcome.rs` | the end of one main-terminal re-attach attempt: `ReattachOutcome` (`Reattached`, `AlreadyAttached`, `Skipped(ReattachSkip)`) with `ReattachSkip` naming why nothing was attached (`InvalidSessionId`, `UnreadableRecord`, `NotTmuxRun`, `TmuxSessionGone`, `TmuxProbeFailed`, `NoSessionType`, `AgentUnresolved`, `AttachFailed`); `CliSessionManager::announce_reattach` logs it at `info` under `tddy_cli_sessions::reattach` and broadcasts a `ReattachFinished { session_id, outcome }` (`REATTACH_CAPACITY`), taken with `reattach_finished()` — so a caller waits on the outcome instead of polling its effects |
| `session_toolcall.rs` | the per-session toolcall listener and the managed-workflow wiring for claude-cli sessions: a `WorkflowController` positioned at the recipe's start goal behind a per-session `transition` handler, and `ManagedWorkflow` |

The argv of every spawn comes from the [CLI harness registry](../../tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness):
`build_claude_argv` and `build_cursor_argv` delegate to the `claude-cli` and `cursor-cli` descriptors'
`argv` (byte-identical to the builders they replaced — `tests/cli_harness_parity.rs`), and
`launch_cli_harness(harness, session_id, worktree, &CliLaunch, HarnessLaunchExtras { args, env, on_exit })`
spawns any registered harness, its route's arguments (`args`) after the descriptor's argv and its route's
environment (`env`) on the child.

## Which sessions are live

`live_agent_sessions()` (`terminals.rs`) returns the sessions this manager holds a live **agent** for
as a `tddy_daemon_kernel::live_sessions::LiveSessions` snapshot: every session with a terminal that
still resolves and is either the `main` terminal or of an agent's kind — not `"bash"`, not `"tmux"` —
plus every session a detach watcher holds. A terminal resolves only while its PTY is registered, so an
agent that exited, or a tab stopped, leaves the set. `CliSessionManager` implements `HeldSessions`
(`live_sessions`, `stop_session`) for surfaces that are handed the daemon's processes through the port.
See [Session liveness](../../../docs/ft/daemon/session-liveness.md).

## tmux terminals

`start_tmux_terminal(session_id, worktree, os_user, argv, tmux_session)` (`terminals.rs`) spawns a
tmux client — the argv the daemon's roster built — through `spawn_tool` with `os_user: Some(_)`, so it
runs as the session's OS user and reaches that user's own tmux server. It is the only started terminal
that is not the daemon user. Its kind is `"tmux"` (`tddy_terminal_rpc::service::TMUX_KIND`), and the
tmux session name is recorded on `PtyHandle::tmux_session` and on the terminal's index entry, so a
listing — or a handle rebuilt from the index — still names it. Stopping it ends the client, which
detaches it.

`kill_all` (daemon shutdown) closes the detach-watcher registry, empties the index
(`drain_terminals`), cancels every task and SIGKILLs what outlives 5 s. A tmux-run main is only a tmux
client, so cancelling it **detaches** it and the agent's tmux session keeps running; a tab's tmux
session is the user's and is never touched. An ended terminal is classified (`tmux_reattach.rs`) when
it is the main or a client of an agent's tmux session — a tmux-wrapped agent tab; a shell, and a tab
attached to the user's own tmux session (kind `"tmux"`, which carries no tmux run), are clients of
nothing the session keeps and are not classified. Only a **main's** end drops the session's managed
workflow and exposure or announces an agent end; a tab's agent ending leaves the session's own agent
state alone, and a tab client that ends while its agent lives hands the tab to its own detach watcher.

`with_claude_launch_flags(argv, initial_prompt, append_system_prompt_file, extra_args)` (`argv.rs`)
inserts `--append-system-prompt-file` and the extra arguments before any positional prompt; the PTY
launch (`start_with_options`) and the tmux launch (`tddy-agent-launch`'s `claude_cli_tmux`) both build
`build_claude_argv` through it, so the two cannot diverge.

## Exit hooks

`on_exit: Option<ExitHook>` is work to finish once the process has exited. `ExitHook`
(`cli_session_manager/pty_spawn.rs`, re-exported from `cli_session_manager`) is a boxed `FnOnce`
receiving the terminal's capture (`Arc<Mutex<TerminalCapture>>`) and returning a future. `spawn_tool`
takes it at spawn, so a fast exit cannot slip past it. The terminal-cleanup task runs it once the process
has exited **and** its output is complete (the PTY runtime reports the exit only after its reader has
drained, see `tddy-pty`), and removes the terminal from the index only once the future has completed.

`CliSessionManager::exit_hook_settled(session_id)` waits for a pending hook of the session's main
terminal when its process has exited. It returns at once when no hook is pending, or when the process is
still running — "still running" meaning its PTY control is still in the PTY registry, where the runtime
registers it before the ready signal and from which it drops it the moment the process exits. A resume
calls it before reading what the previous run's hook recorded (`tddy-agent-launch`'s resume-token
recorder).

Tests: `tests/exit_hook.rs` — `a_resume_waits_for_the_exit_hook_of_the_run_before`,
`a_resume_does_not_wait_for_a_run_that_is_still_going`.

## Recording the main terminal

`cli_session_manager/capture_recorder.rs` records a session's `main` terminal to disk so its history
outlives the process. `record_main_capture(session_id, path)` registers the running `main` (a session
with none is logged and not registered; registering again replaces, and closes, the earlier
registration). While `main` runs its capture is rewritten to `path` every `RECORDING_INTERVAL` (30 s,
`tddy_task`; `with_recording_interval` overrides it), through `CaptureRecording::record`. The final
write is the PTY runtime's **exit hook** (`tddy_pty::PtyExitHook`, installed by `spawn_tool` for the
`main` terminal only), which runs before the terminal leaves the PTY registry — so a stopped `main`
that can no longer be found running already has a complete recording. A `closed` flag under one async
mutex keeps a periodic write from landing after the exit write.

`main_capture_recording_path(tddy_data_dir, session_id)` (`<data dir>/sessions/<id>/terminals/main.capture`,
`MAIN_CAPTURE_RECORDING`) lives here, beside the writer, so `tddy-agent-launch` can compute it without
depending on `tddy-session-lifecycle`; the readers re-export it. `tddy-agent-launch` registers the
recording on start and on resume.

Tests: `tests/capture_recorder.rs` (a real PTY, a 50 ms interval) —
`a_registered_main_is_recorded_with_its_last_output_before_it_leaves_the_index`,
`a_running_main_is_recorded_on_the_interval`, `an_unregistered_session_is_never_recorded`.

`tddy_session_lifecycle` re-exports both modules (`cli_session_manager`, `session_toolcall`) and keeps
`claude_cli_session` as a historical alias of `cli_session_manager`.

## Process tabs

`cli_session_manager/process_tab.rs` holds a PTY-less `main` tab for a tool session's `tddy-coder`:
`register_process_tab(session, kind, pid, exit, holder)`, `process_tab`, `signal_process_tab` and the
crate-private `stop_process_tab` (called by `stop_session`). A tab counts in `live_agent_sessions`, is
absent from `list_terminals`, and leaves when its `tddy_task::ExitReceiver` resolves; a generation
number keeps a late exit from removing the tab of a resumed child. `ProcessTabHolder` (`stop`, `signal`)
is implemented beside each spawn backend, in `tddy-agent-launch`. `ChildExit` and the exit channel live
in `tddy-task`, so this crate does not depend on the spawn backends.

## Agent and user process environments

`tddy_pty::PtySpawnSpec.env_base` is `PtyEnvBase::{Inherited, Cleared}`. `spawn_rendered_main` — every
agent main — spawns on `Cleared`: the child sees exactly the spec's env (the daemon-computed agent env)
plus the PTY's own `TERM=xterm-256color` and `COLORTERM=truecolor`, which stand over any the spec names.
`spawn_tool` and the user's terminals (Bash, tmux clients) stay `Inherited`: a user's terminal is not an
agent, and still sees the daemon's environment. `spawn_tool_on(env_base, exit_code_in_output, …)` is
the shared spawn both reach. `spawn_rendered_client_main` is `spawn_rendered_main` for a main whose
process is a transport's client to an agent on a target: its `tddy_pty::ExitCodeInOutput` says how the
agent's status is read from what a client that exits with a constant code (Coder's `coder ssh`, 1)
printed last, so the terminal's status — and the `main exited` line and `AgentEnd` built from it — is
the agent's own. `spawn_rendered_client_tab(session_id, terminal_id, worktree_path, ClientTab)` is its sibling
for a **tab** (`#session-liveness` 6/8): the same transport client, registered under the tab's id, never `main`
(refused for `main` and for an id already live), so nothing of the session's main — its recording, its exit hook,
its status — is touched and the client's exit ends the tab alone; it takes `spawn_tool_on`'s existing parameters.
Tests: `tests/agent_env_allow_list.rs` (`an_agent_main_spawned_through_spawn_rendered_main_sees_no_unlisted_daemon_variable`,
and the guard `a_user_terminal_still_inherits_the_daemon_environment`).