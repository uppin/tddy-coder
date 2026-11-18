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
- `tddy-tool-engine`: possibly a narrow `pub` re-export of the screen-region helpers, if they are
  not reachable as they are.
- `Cargo.toml` (workspace members); `./release`, `./install` and `scripts/dev-runtime-binaries.sh`
  are **not** changed. This is a developer tool and is not installed beside the daemon.

## Scope

- [ ] Arguments: `--target`, `--worktree`, `--agent`, `--model`, `--prompt`, `--binary`, `--env`,
      `--start`
- [ ] Launch through `launch_host_pty` (local) and `launch_on_target` (ssh, coder)
- [ ] The six MCP tools over stdio
- [ ] Acceptance suite (local and stand-in Coder) driven over MCP as a subprocess
- [ ] Live use on `mantasi/remote-dev`
- [ ] Scoped gates

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
