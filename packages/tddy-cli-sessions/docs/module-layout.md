# `tddy-cli-sessions`: module layout

The daemon's CLI session PTY runtime: PTY-hosted `claude`, `cursor` and `opencode` sessions and their terminals. About
**1.7k production lines** in 12 modules; no function over 100 lines. It depends on neither
`tddy-session-split`, `tddy-agent-launch` nor `tddy-session-agents`.

| Module | Holds |
|---|---|
| `cli_session_manager.rs` | `CliSessionManager`, the PTY session manager and the origin of `TaskRegistry`; the struct, `ControlLeaseInfo` and the per-session exposure record |
| `cli_session_manager/pty_handle.rs` | `PtyHandle`, `send_input` |
| `cli_session_manager/control_lease.rs`, `argv.rs`, `launch.rs`, `pty_spawn.rs`, `relaunch.rs`, `terminals.rs`, `exposed_terminals.rs` | `impl CliSessionManager` blocks: the control lease, the argv a spawn builds, launch, the PTY spawn, relaunch, the local terminals, and the exposure of a session's terminal through a `TerminalExposure` (`ensure_exposed_terminal`, single-flight per session: each recorded terminal carries its own gate, held across the join, while the registry's lock is taken only to read and to write the record, so a slow join to one session holds up neither another session's join nor any other registry access) |
| `cli_session_manager/pty_terminal_service.rs` | `pty_terminal_service(handle)`: serves `terminal.TerminalService` against a PTY handle, built from `tddy-rpc` types only, so any transport can host it |
| `session_toolcall.rs` | the per-session toolcall listener and the managed-workflow wiring for claude-cli sessions: a `WorkflowController` positioned at the recipe's start goal behind a per-session `transition` handler, and `ManagedWorkflow` |

The argv of every spawn comes from the [CLI harness registry](../../tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness):
`build_claude_argv` and `build_cursor_argv` delegate to the `claude-cli` and `cursor-cli` descriptors'
`argv` (byte-identical to the builders they replaced — `tests/cli_harness_parity.rs`), and
`launch_cli_harness(harness, session_id, worktree, &CliLaunch, extra_args, env)` spawns any registered
harness, its route's arguments after the descriptor's argv and its route's environment on the child.

`tddy_session_lifecycle` re-exports both modules (`cli_session_manager`, `session_toolcall`) and keeps
`claude_cli_session` as a historical alias of `cli_session_manager`.
