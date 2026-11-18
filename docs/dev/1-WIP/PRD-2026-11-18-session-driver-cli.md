# PRD: A CLI that runs one agent session on a local, ssh or Coder target and is driven over MCP

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 3/3 — branch `feature/coder-start/session-driver-cli`, base
`feature/coder-start/interactive-launch` (#941)

## Problem

Proving that an agent session really starts and runs on a target took temporary Rust tests:

- a hand-built `launch_on_target` call;
- an ANSI stripper;
- `sleep` loops over the PTY capture;
- keys typed into a dialog by matching de-spaced screen text.

Each was written, run against `mantasi/remote-dev`, and deleted. The daemon's orchestrator tools
(`ReadSessionTerminal`, `ClaimSessionTerminal`, `SendSessionKeys`) can drive a session's TUI, but
only a session a running daemon started, through a project-home session, with a Coder account in
that daemon's vault. None of that is available when the point is to check the launch path itself, or
a branch that is not yet installed.

## Solution

A binary, **`tddy-session-driver`**, that starts **one** agent session through the daemon's own launch
code on the target its arguments name. By default it serves an MCP server on its **stdin/stdout**, so
an agent (or a test) drives the session's TUI with tools modelled on the orchestrator's terminal
tools. The TUI is not shown.

```
tddy-session-driver --target local                         --worktree <dir> --agent claude [--model sonnet] [--prompt …]
tddy-session-driver --target ssh:<alias>                   --worktree <path on target> --agent claude …
tddy-session-driver --target coder:<owner>/<workspace>[.<agent>] --worktree <path on target> --agent claude …
```

- **Agents** resolve as the daemon resolves them: the built-in specs plus `<tddy home>/tools`, on this
  host (`host_coding_agents`).
- **`--binary <path>`** and repeated **`--env KEY=VALUE`** name the agent's binary and environment on
  the target, for as long as a target launch runs this host's resolved path. That is the backlog entry
  `2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path`, which this flag works around
  explicitly and does not fix.
- **Coder credentials** come from the CLI's own variables, `CODER_URL` and `CODER_SESSION_TOKEN`.
  No vault is involved. A Coder target with either variable missing is refused before anything runs.
- **The launch** goes through `launch_host_pty` for `local` and `launch_on_target` for `ssh:` /
  `coder:`. These are the code paths a daemon start runs, so what the driver sees is what a daemon
  start does.
- **The worktree** must already exist. Cutting one is not this binary's job; a Coder or ssh worktree
  can be cut with `tddy_git::setup_remote_worktree` or by hand.

### MCP tools (stdio)

| Tool | What it does |
|---|---|
| `start_session` | Launches the session the CLI arguments describe. Answers `started` with the launch's time, or the launch's failure verbatim (`ExitedDuringStartup` with the last output, `AgentNeverStarted`, a shell error). A second call while running is refused. |
| `read_terminal` | The session's screen as `ReadSessionTerminal` answers it: lines, cursor, cols/rows, offset, and optionally `styles` and `region`. Same semantics, no `session_id`. |
| `send_keys` | `text` and/or named `keys`, encoded as `SendSessionKeys` encodes them. No lease: the driver's session has one controller. |
| `wait_for_text` | Waits until the screen (text with whitespace removed, so cursor-positioned TUIs match) contains `text`, or `timeout_secs` passes. Answers the screen either way, and whether it matched. |
| `session_status` | `not_started` / `running` / `exited` with its status and last output. |
| `stop_session` | Stops the session's terminal and, on a remote target, the agent process the launch recorded there. |

With `--start` the session is launched at once, before the MCP server answers. Without it, the first
`start_session` call launches it.

## Acceptance criteria

- [ ] Over MCP stdio against a **local** target, a stub agent is started:
  - `wait_for_text` sees its banner;
  - `send_keys` input reaches it and `read_terminal` shows the echo;
  - `stop_session` ends it, and `session_status` reports `exited`.
- [ ] The same against a **Coder** target through a stand-in `coder`, with `CODER_URL` and
      `CODER_SESSION_TOKEN` taken from the environment. The stand-in records that the CLI was given
      them.
- [ ] A Coder target without `CODER_SESSION_TOKEN` is refused before anything runs, naming the
      variable.
- [ ] A launch failure (an agent that exits at once) comes back from `start_session` with the agent's
      last output.
- [ ] stdout carries only MCP frames; diagnostics go to stderr.
- [ ] Scoped gates for the new package and the packages it touches.
- [ ] Used live: drives a Claude session on `mantasi/remote-dev` through the folder-trust dialog to a
      completed turn, as the deleted temporary tests did.

## Out of scope (node 4 of `#coder-start`, not planned yet)

- An MCP endpoint on a TCP port or a unix socket instead of stdio.
- Showing the TUI on the driver's own terminal while MCP is served on a socket.
- Cutting worktrees, and more than one session per process.
