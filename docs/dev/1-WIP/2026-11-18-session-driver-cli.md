# Changeset: A CLI that runs one agent session on a local, ssh or Coder target and is driven over MCP

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4.
- A new binary package.
- Three target kinds, each through the daemon's existing launch path.
- An `rmcp` stdio server of six tools whose screen and key semantics mirror the orchestrator's
  terminal tools.
- Acceptance tests drive it as a subprocess over MCP.
**Stack**: `#coder-start` 3/3 — branch `feature/coder-start/session-driver-cli`, base
`feature/coder-start/interactive-launch` (#941)

## Initial Discovery

[`2026-11-18-session-driver-cli-initial-discovery.md`](2026-11-18-session-driver-cli-initial-discovery.md)

## Prerequisites

### ℹ Worked around, not fixed — the target launch runs the daemon host's agent path — [`2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md`](../todo/2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md)

`--binary` / `--env` name the target's binary and environment explicitly. The entry stays open: the
daemon still has no way to choose them.

### ℹ Not this node — the folder-trust and project MCP dialogs — [`2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt.md`](../todo/2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt.md)

The driver lets a caller answer them (`wait_for_text`, then `send_keys`). Nothing answers them
automatically.

### ⚠ DURING — target-session test fixtures are duplicated — [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)

The driver's acceptance suite needs a stand-in `coder` and a stub agent too. It takes the stand-in
from `tddy-remote-shell`'s test support where it can, rather than adding an eighth copy.

## Affected Packages

- **New:** `tddy-session-driver`, a binary plus a small library for its tools.
  - Dependencies, all already in the workspace: `rmcp` (as `tddy-tools` uses it), `clap`, `tokio`,
    `serde_json`, `tddy-agent-launch`, `tddy-remote-shell`, `tddy-task`, `tddy-tool-engine` (for
    `encode_keys`) and `tddy-cli-sessions`.
- `tddy-tool-engine`: `terminal_grid_answer` is public, the screen answer `ReadSessionTerminal` gives,
  taking a `GridRegion`.
- `tddy-agent-launch`: `REMOTE_AGENT_PID_FILE` is public and re-exported from `remote_agent_launch`.
- `Cargo.toml` (workspace members); `./release`, `./install` and `scripts/dev-runtime-binaries.sh`
  are **not** changed. This is a developer tool and is not installed beside the daemon.

## Scope

- [x] Arguments: `--target`, `--worktree`, `--agent`, `--model`, `--prompt`, `--permission-mode`,
      `--binary`, repeated `--env`, `--start`, plus:
  - `--tddy-home` (default `~/.tddy`);
  - `--coder` (default `coder`, found on `PATH`);
  - `--startup-window-secs` (default 5);
  - `--rows` / `--cols` (default 40×120). The screen model answers nothing until the PTY's size is
    known, so the driver sizes it right after the launch.
- [x] Launch through `launch_host_pty` (local) and `launch_on_target` (ssh, coder). A local start is
      judged by the same startup window, here, since `launch_host_pty` does not wait.
- [x] The six MCP tools over stdio, `rmcp` 1.x.
  - `read_terminal` answers through `tddy_tool_engine::project_home_terminal_tools::terminal_grid_answer`,
    made public for it; it was the private `grid_answer`, now taking a `GridRegion`.
  - `send_keys` encodes through `encode_keys`, bracketed paste included.
  - `stop_session` kills the remote agent by the pid file the launch recorded.
    `REMOTE_AGENT_PID_FILE` is `pub`, re-exported from `remote_agent_launch`.
- [x] Acceptance suite `tests/session_driver_acceptance.rs`, 7 tests. It spawns the compiled binary
      and speaks newline-delimited JSON-RPC, failing on any stdout line that is not a frame:
  - the tool list and `not_started`;
  - a local conversation: banner, keys, echo on the screen, stop, `exited`;
  - the same through a stand-in `coder` given `CODER_URL` / `CODER_SESSION_TOKEN`;
  - a Coder target without a token exits 1 naming the variable, with the CLI never run;
  - an agent that exits at once fails `start_session` with its last words;
  - `--start`;
  - stopping a Coder session terminates its agent on the workspace by the recorded pid. This one was
    added at the 6e review: the stand-in `coder` waits on the command instead of becoming it, and
    the agent ignores SIGHUP, so only the remote kill ends it. It fails with that kill disabled.
  The first six failed against a stub binary before the implementation.
- [x] Unit tests: target parsing and the Coder variable refusal (`target.rs`, 3).
- [x] Live use on `mantasi/remote-dev` (2026-11-18), driven over MCP stdio by a ~60-line Python
      client, with `--binary /home/coder/.local/bin/claude --env HOME=… --env PATH=… --env TMPDIR=/tmp`:
  - `start_session` → `running` after 44 s;
  - Claude's turn completed on screen: `hostname: coder-mantasi-remote-dev`, and `pwd:` the fresh
    worktree;
  - `stop_session` → `exited`, and no `claude` was left on the VM.
  - No trust dialog appeared in that fresh worktree. Trusting an earlier qape-hq worktree trusted the
    repository.
  - Without `--env TMPDIR=/tmp` the launch failed `EACCES: permission denied, mkdir '/var/folders'`:
    the agent environment carries this Mac's `TMPDIR` to the target. That is recorded on the backlog
    entry for the daemon host's agent environment.
- [x] Scoped gates:
  - `./test -p tddy-session-driver -p tddy-tool-engine -p tddy-agent-launch` exits 0, with 0 failures
    in every suite;
  - `./dev cargo clippy -p tddy-session-driver -p tddy-tool-engine -p tddy-agent-launch --all-targets
    -- -D warnings` is clean.

## Responsibility

One agent session, launched through the daemon's own code on a named target, and driven over MCP
from outside.

## Boundaries

- No change to how the daemon launches anything.
- No worktree cutting.
- No socket or port transport, and no visible TUI (node 4, unplanned).
- No automatic answer to any agent dialog.
- No Cursor chat minting on a target: a `cursor` agent on a remote target fails exactly as a daemon
  start does.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #939 | Coder one-shots that cross the pty intact | the launch's slot writes and probes | change `CoderShell::exec` |
| #941 | A Coder/ssh terminal that opens (the daemon's `PATH` for the client, no `-t`), and start probes bounded by the start's own time | `launch_on_target` reaching a live workspace | touch `launch_on_target`, `startup.rs` or `CoderShell::interactive` |

## Draft PR contract

The draft opens with these planning documents. The binary's arguments, the tool names and schemas
above, and the failing acceptance suite follow in this PR. Nothing downstream consumes them yet.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** yes, for its suite. The stand-in `coder` runs commands on this machine,
so the suite needs only the launch code that exists today. Its **live** use needs #941, without which
a Coder terminal never opens.
**Concurrent with:** #941's review
**Blocks:** none
