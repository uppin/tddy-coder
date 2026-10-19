# `tddy-cli-sessions`: module layout

The daemon's CLI session PTY runtime: PTY-hosted `claude` and `cursor` sessions and their terminals. About
**1.7k production lines** in 12 modules; no function over 100 lines. It depends on neither
`tddy-session-split`, `tddy-agent-launch` nor `tddy-session-agents`.

| Module | Holds |
|---|---|
| `cli_session_manager.rs` | `CliSessionManager`, the PTY session manager and the origin of `TaskRegistry`; the struct, `ControlLeaseInfo` and the per-session exposure record |
| `cli_session_manager/pty_handle.rs` | `PtyHandle`, `send_input` |
| `cli_session_manager/control_lease.rs`, `argv.rs`, `launch.rs`, `pty_spawn.rs`, `relaunch.rs`, `terminals.rs`, `exposed_terminals.rs` | `impl CliSessionManager` blocks: the control lease, the argv a spawn builds, launch, the PTY spawn, relaunch, the local terminals, and the exposure of a session's terminal through a `TerminalExposure` (`ensure_exposed_terminal`, single-flight under the registry's write lock) |
| `cli_session_manager/pty_terminal_service.rs` | `pty_terminal_service(handle)`: serves `terminal.TerminalService` against a PTY handle, built from `tddy-rpc` types only, so any transport can host it |
| `session_toolcall.rs` | the per-session toolcall listener and the managed-workflow wiring for claude-cli sessions: a `WorkflowController` positioned at the recipe's start goal behind a per-session `transition` handler, and `ManagedWorkflow` |

`tddy_session_lifecycle` re-exports both modules (`cli_session_manager`, `session_toolcall`) and keeps
`claude_cli_session` as a historical alias of `cli_session_manager`.
