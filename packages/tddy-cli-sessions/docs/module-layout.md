# `tddy-cli-sessions`: module layout

The daemon's CLI session PTY runtime: PTY-hosted `claude`, `cursor` and every other registered CLI harness's sessions (`opencode`, `muse`, `warp`) and their terminals. About
**1.7k production lines** in 12 modules; no function over 100 lines. It depends on neither
`tddy-session-split`, `tddy-agent-launch` nor `tddy-session-agents`.

| Module | Holds |
|---|---|
| `cli_session_manager.rs` | `CliSessionManager`, the PTY session manager and the origin of `TaskRegistry`; the struct, `ControlLeaseInfo` and the per-session exposure record |
| `cli_session_manager/pty_handle.rs` | `PtyHandle`, `send_input` |
| `cli_session_manager/control_lease.rs`, `argv.rs`, `launch.rs`, `pty_spawn.rs`, `relaunch.rs`, `terminals.rs`, `exposed_terminals.rs` | `impl CliSessionManager` blocks: the control lease, the argv a spawn builds, launch, the PTY spawn, relaunch, the local terminals, and the exposure of a session's terminal through a `TerminalExposure` (`ensure_exposed_terminal`, single-flight per session: each recorded terminal carries its own gate, held across the join, while the registry's lock is taken only to read and to write the record, so a slow join to one session holds up neither another session's join nor any other registry access) |
| `cli_session_manager/pty_terminal_service.rs` | `pty_terminal_service(handle)`: serves `terminal.TerminalService` against a PTY handle, built from `tddy-rpc` types only, so any transport can host it. Every open starts with a snapshot frame taken under the capture lock (`tddy_terminal_rpc::session::snapshot_and_subscribe`), then live frames stamped with contiguous offsets; a `request_snapshot` after the opening message, or a lagged receiver, re-takes the snapshot at a new sequence point. No raw replay and no `trigger_redraw` — `PtyHandle::trigger_redraw` remains for the host's legacy bridge |
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
