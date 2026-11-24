# tddy-session-driver architecture

## Overview

A **developer tool**: one binary, `tddy-session-driver`, that starts **one** agent session through
the daemon's own launch code on a named target, and serves an MCP server on its stdin/stdout so an
agent (or a test) drives the session's TUI. It exists to check a target launch live — on a branch
that is not installed, with no running daemon, no project-home session and no vault — which before
it took temporary Rust tests written, run and deleted per check.

It is **not** shipped: `./release`, `./install`, `./publish.sh` and
`scripts/dev-runtime-binaries.sh` do not build or install it. Run it from the checkout:

```bash
cargo run -p tddy-session-driver -- --target coder:<owner>/<workspace> --worktree <path> --agent claude …
```

## Usage

```
tddy-session-driver --target local                               --worktree <dir>            --agent claude [--model sonnet] [--prompt …]
tddy-session-driver --target ssh:<alias>                         --worktree <path on target> --agent claude …
tddy-session-driver --target coder:<owner>/<workspace>[.<agent>] --worktree <path on target> --agent claude …
```

| Argument | Meaning |
|---|---|
| `--target` | `local`, `ssh:<Host alias>`, or `coder:<owner>/<workspace>[.<agent>]` (`target.rs`) |
| `--worktree` | The agent's working directory, on the target. It must exist; the driver cuts none |
| `--session-id` | The session's id (default: a fresh UUID). A Claude start pre-trusts the clone only when the worktree is `<clone>/.worktrees/<session id>`, so a caller that lays out that layout names the id here. Claude requires a UUID |
| `--agent` | The coding agent's id, resolved as the daemon resolves it: built-in specs plus `<tddy home>/tools` (`host_coding_agents`) |
| `--model`, `--prompt`, `--permission-mode` | Passed into the harness template as a session start passes them |
| `--binary <path>` | The agent's binary, replacing the spec's: set as both the tool's resolved binary (what a local launch runs) and its command (what a target launch runs on the target's `PATH`) |
| `--env KEY=VALUE` | Repeatable; set on the agent on top of its spec's environment |
| `--tddy-home` | Whose `tools/` the agents are read from (default `~/.tddy`) |
| `--coder` | The `coder` CLI a Coder target is reached through (default `coder`, on `PATH`) |
| `--startup-window-secs` | How long the agent must stay up for its start to count (default 5) |
| `--rows`, `--cols` | The terminal size (default 40×120). The screen model answers nothing until the PTY's size is known, so the driver sizes it right after the launch |
| `--start` | Launch before serving; a failed start exits 1. Without it, the first `start_session` launches |

A **Coder** target reads its deployment and token from the CLI's own variables, `CODER_URL` and
`CODER_SESSION_TOKEN`. Either missing (or empty) is refused before anything runs, exit 1, naming
the variable.

### The launch

- `local` → `launch_host_pty`. That call does not wait, so the driver judges the startup window
  itself the way `launch_on_target` does: an agent whose PTY ends inside it failed to start.
- `ssh:` / `coder:` → `launch_on_target` with `SshShell` / `CoderShell::through`, the code path a
  daemon target start runs. The session directory (the remote client's local cwd) is
  `$TMPDIR/tddy-session-driver/<session id>`.

**`--binary` and `--env` are overrides, not workarounds.** A target launch runs the target's own
agent: its bare command on the target's login `PATH`, with the target's `HOME`, `PATH` and `TMPDIR`
(`tddy_agent_launch::remote_agent_launch::target_login_exec`). The flags only replace that binary or
add variables for a one-off check. The driver also passes the worktree's clone root
(`clone_root_of_worktree`), so a Claude start pre-trusts it on the target as a daemon start does.

## The MCP tools

Served with `rmcp` 1.x on stdio (`server.rs`). **stdout carries only MCP frames**; every diagnostic
goes to stderr. Screen and key semantics are the orchestrator's own: `read_terminal` answers through
`tddy_tool_engine::project_home_terminal_tools::terminal_grid_answer` (what `ReadSessionTerminal`
answers) and `send_keys` encodes through `tddy_tool_engine::terminal_keys::encode_typing` and types
through `type_into` (what `SendSessionKeys` does): `DriverSession` implements `TypingTerminal`, its
output offset the screen grid's `offset` and each write a `send`, so keys after text are pressed once
the agent has echoed the text, or after 5 s with no echo.

| Tool | Answers |
|---|---|
| `start_session` | `{state: "running", after_ms}`, or the launch's failure verbatim — for an agent that exited while starting, its exit and last screen lines. A second start is refused: one driver, one session |
| `read_terminal` | `{lines, cursor, cols, rows, offset}`; `styles: true` adds `styles` and `highlighted_rows`; `region {row, col, rows, cols}` crops to that box |
| `send_keys` | `{sent_bytes, echo_seen?}` for `text` then named `keys` (bracketed paste when the terminal enabled it; the keys once the agent has echoed the text). `echo_seen` is answered when keys followed text: `false` means no echo came within 5 s and the keys were pressed anyway. No lease. Nothing is sent for an unknown key name or an exited session; a failure after the text was written says the keys were not |
| `wait_for_text` | `{matched, exited, lines}`, once the screen contains `text` with whitespace ignored on both sides (TUIs place words by cursor moves), the session exits, or `timeout_secs` (default 30) passes |
| `session_status` | `{state: "not_started"}`, `{state: "running"}`, or `{state: "exited", exit, last_output}` / `{state: "exited", failure}`. On a target `exit` is the agent's own status, read through the transport (`InteractiveExit`) — on Coder, from the CLI's last line rather than its constant 1 |
| `stop_session` | `{state: "exited"}` once the terminal ended. On a remote target it first kills the agent by the pid this session's launch recorded at `<worktree>/.tddy/agents/<session id>.pid` (`remote_agent_pid_file`), and removes that file in the same command, since ending the local client does not end the agent there. Sessions sharing a worktree each stop only their own agent |

| `read_hooks` | `{followed, hooks, next}` from `since` (default 0) on: each hook `{index, event, payload, reports}`, its reports the `ReportSessionStatus` / `ReportAgentActivity` requests the daemon's activity handling would be served, as `{kind: session_status \| agent_activity, …}` without the hook token. `followed` is false for an agent without hooks or a session not started; `next` is how many hooks arrived |
| `wait_for_hook` | `{matched, hook}` once a hook of `event` arrives (from `since` on), or after `timeout_secs` (default 30). Refused for a session whose hooks are not followed |

When the MCP client goes away the driver stops the session before it exits.

## Hooks

A session whose agent's spec declares the `hooks` capability has its lifecycle hooks wired and
followed exactly as a daemon start on a target wires and follows them (`hooks.rs`), on every target
— a local one included:

- a fresh hook token, and the hook slots rendered with `HookCommandParams::remote_log` set to
  `tddy_core::REMOTE_HOOK_LOG`, so every hook is the shell one-liner that appends
  `<event>\t<token>\t<payload>` to `.tddy/hooks.log` in the worktree; the hooks file is written into
  the worktree with the launch's other slot files (through the port on a target);
- the log created before the launch with `remote_hook_follower::create_remote_hook_log`, through the
  session's shell — the target's, or `tddy_remote_shell::LocalShell` for a local session;
- once the agent is up, a `RemoteHookFollower` on that shell, registered in a `RemoteHookFollowers`,
  each line mapped by the daemon's `ActivityHookSink`;
- the session's exit stopping the follower and removing the log, as a daemon target session's does;
  a stop returns once both are done, and a start that fails removes the log it created.

Only the delivery differs. A daemon serves each report to its activity service; the driver's
`HookReportDelivery` records it beside the hook that raised it, for `read_hooks` and `wait_for_hook`.
The driver runs no orchestrator: its `await_question_answer` answers `NO_ORCHESTRATOR`, and the sink
answers through the session's shell, so a waiting `PermissionRequest` hook gets an empty answer file
at once and the agent asks in its own terminal instead of after its whole answer wait.

Agent dialogs (Claude's folder trust, a project's MCP servers) are not answered automatically; a
caller answers them with `wait_for_text` then `send_keys`.

## Modules

- `target.rs` — `DriverTarget` parsing and the `RemoteShell` that reaches it; the Coder variable refusal.
- `session.rs` — `DriverSession`: launch, screen, keys, waits, stop, over one `CliSessionManager`.
- `hooks.rs` — `SessionHooks` (token, log, follower), `HookRecorder`, and the recording sink and
  delivery behind `read_hooks` / `wait_for_hook`.
- `server.rs` — `DriverServer`, the eight `#[tool]`s.
- `main.rs` — `clap` arguments into a `LaunchSpec`, then serve.

## Tests

`tests/session_driver_acceptance.rs` spawns the compiled binary and speaks newline-delimited JSON-RPC,
failing on any stdout line that is not a frame. Local and Coder conversations (through a stand-in `coder`
the suite writes, which runs the workspace's command on this machine), the missing-token refusal, a start failure's last words,
`--start`, a Coder stop that must kill the agent by its recorded pid and remove the file, two sessions
in one worktree that each stop only their own agent, `--binary` reaching a Coder workspace, and a
session behind a stand-in that exits 1 as the real CLI does, whose status must still be the agent's. Unit tests in `target.rs`
cover parsing and the refusal; unit tests in `main.rs` cover the `--session-id` default.
The machine and the MCP client the suites share live in `tests/support/`: the `acme` and
`acme-hooked` tool files, the stand-in `coder`, and a stand-in `ssh` first on the driver's `PATH` for
`CoderShell::exec_streamed` — it checks it was handed `coder ssh --stdio` as its proxy and runs the
command as a child it waits on, so killing the client leaves the command running, as a real ssh
session does. `write_executable` comes from `tddy_testing_commons::remote_target`.

`tests/session_driver_hooks_acceptance.rs` fires hooks as Claude Code does — the command installed
in the session's hooks file in the worktree (`.tddy/hooks/<session_id>/claude-settings.json`),
under `sh -c`, payload on stdin — on a Coder and a
local session, and checks what `read_hooks` and `wait_for_hook` answer: the daemon's reports, in
order, a forged line dropped, the Coder log read through the CLI, and after the stop the log gone
and nothing left following it. Also `since`, a wait that times out, and an agent without hooks.

`tests/coder_live_production.rs` is a production test (`#[ignore]`, run with
`./test -p tddy-session-driver -- --ignored coder_live`) against a real Coder deployment. It needs
`TDDY_CODER_LIVE_WORKSPACE`, `CODER_URL` and `CODER_SESSION_TOKEN` and fails, never skips, when one is
missing; the token is inherited by the `coder` children and never put on an argument list. It checks
that `coder version` and the workspace status read as the stand-ins assume, that Cursor mints its chat
and reaches `Cursor Agent`, and that Claude on a cloned worktree (`--session-id`) reaches `❯ Try` with no
trust dialog.
