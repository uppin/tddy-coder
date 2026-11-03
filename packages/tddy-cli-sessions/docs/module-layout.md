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
| `tmux.rs` | the host's tmux, everything **run as the session's OS user** through the kernel's `run_output_as_user`: `tmux_binary` (`TDDY_TMUX_BINARY`, else `tmux`), `tmux_attach_argv` (`-T clipboard attach-session -t =<name>`), `tmux_session_name` / `tmux_new_session_argv` (the `tmux-new` tab), `list_tmux_sessions` + `parse_list_sessions` ("no server running" is `[]`), `ensure_reachable` (refuses a user this daemon cannot become); and the agent's tmux session — `agent_tmux_session_name` (`tddy-<full session id>`, `:`/`.` → `_`), `agent_new_session_args` / `start_agent_tmux_session` (`new-session -d -s -c -- <pane command>`; returns the pane pid; the pane's environment — the user's `HOME`/`PATH` under the session env — goes through `tmux/pane_env.rs`: a fresh `0600` file under `<session dir>/tmux-env/` (`0700`), both handed to the session's OS user when the daemon is another, written as shell-quoted `export K='v'` lines; the pane runs `/bin/sh -c '. "$1"; rm -f -- "$1"; shift; exec "$@"' sh <file> <argv>`, so no value is on any argv; a failed start removes the file), `kill_tmux_session` (an absent session or server is not an error), `tmux_session_alive`, `pane_pid` (`=<name>:`); with `remain_on_exit` the start sets `remain-on-exit` on the pane (`keep_pane_after_exit`; a failure ends the session). `tmux/pane.rs` (re-exported) is the kept pane after the agent exits: `remain_on_exit_args`, `capture_pane_args` / `capture_final_screen` (`capture-pane -p -S - -t =<name>:`), `PaneState` / `pane_state` (`#{pane_dead}`; tmux 3.6a answers an absent session with success and no output, read as `SessionGone`). Lives here, below `tddy-agent-launch` and `tddy-session-lifecycle`, so both use it |
| `cli_session_manager/tmux_main.rs` | `attach_main_to_tmux(session_id, worktree, os_user, kind, name)`: the session's **main** terminal as a tmux client of the agent's tmux session, run as `os_user` and recorded with `tmux_session = Some(name)`; an ordinary main, exit hook included |
| `cli_session_manager/tmux_exit_hook.rs` | the exit-hook source of a tmux-run main (`#tmux-agent` 3/3): `run_exit_hook_on_agent_exit(session_id, os_user, name, hook)` starts a watcher polling `pane_state` every `EXIT_HOOK_POLL_INTERVAL` (1 s); once the agent has exited it runs the hook on `capture_final_screen` as the session's pending exit (so `exit_hook_settled` waits on it), then kills the tmux session. One watcher per session (`ExitHookWatchers`); `kill_all` closes the registry and aborts the watchers still polling — never a hook already running. `exit_hook_capture(MainExit, final_screen)` reads the pane only on `AgentEnded` |
| `cli_session_manager/tmux_reattach.rs` | a tmux-run agent outliving its client (`#tmux-agent` 2/3): `classify_main_exit(tmux_session, probe) -> MainExit` (`ClientOnly` when the recorded tmux session is alive, `AgentEnded` otherwise; a PTY-run main never probes) and `classify_ended_main` (the probe on the blocking pool; one that does not finish is `ClientOnly`, so nothing is torn down on a guess); `reattach_main(session_id, os_user, name, worktree, kind)` — `attach_main_to_tmux`, or the main already attached to `name`, stopping the session's watcher; `has_managed_workflow`; and the **detach watcher** `DetachWatch` (`main_ended`: `AgentEnded` drops the managed workflow and the exposure, `ClientOnly` starts a watcher polling `tmux_session_alive` every `DETACH_WATCH_INTERVAL`, 10 s, `with_detach_watch_interval` for tests, which runs the same cleanup once the tmux session is gone). The watcher registry is closed by `kill_all`, so the clients a shutdown cancels start no watcher |
| `session_toolcall.rs` | the per-session toolcall listener and the managed-workflow wiring for claude-cli sessions: a `WorkflowController` positioned at the recipe's start goal behind a per-session `transition` handler, and `ManagedWorkflow` |

The argv of every spawn comes from the [CLI harness registry](../../tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness):
`build_claude_argv` and `build_cursor_argv` delegate to the `claude-cli` and `cursor-cli` descriptors'
`argv` (byte-identical to the builders they replaced — `tests/cli_harness_parity.rs`), and
`launch_cli_harness(harness, session_id, worktree, &CliLaunch, HarnessLaunchExtras { args, env, on_exit })`
spawns any registered harness, its route's arguments (`args`) after the descriptor's argv and its route's
environment (`env`) on the child.

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
session is the user's and is never touched. A main's end is classified (`tmux_reattach.rs`): only the
agent's end — not a client's — drops the session's managed workflow and exposure.

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
