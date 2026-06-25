# WIP changeset — Multiple tools per session (Bash tool, RPC, identified)

**Status:** Green — implemented and passing. `terminal_session_acceptance` 12/12,
`claude_cli_session_acceptance` 12/12 (no regression), clippy `-D warnings` clean. **(Updated: 2026-06-25.)**

**Updated: 2026-06-25** — Reframed from "attachable login-shell terminals" to **running multiple
tools per session**: a session hosts identified tool instances — the Claude CLI tool (reserved id
`"main"`, kind `"claude-cli"`) plus additional tools. This changeset defines the first additional
tool, **Bash**: a shell tool that takes **no inputs** and opens on the session's worktree
directory (kind `"bash"`). The RPC surface keeps its terminal-oriented names
(`StartTerminalSession`/`StopTerminalSession`/`ListTerminalSessions`); each started terminal *is* a
Bash tool instance.

Feature: [daemon/terminal-sessions.md](../../ft/daemon/terminal-sessions.md)

## Scope

Multiple identified tools per tddy session, managed over RPC, reusing the existing `PtyHandle` PTY
mechanic. The **Bash tool** (built-in `$SHELL`, fallback `/bin/bash`, no inputs) attaches to a
session's worktree alongside the main `claude` terminal. The Claude CLI terminal is identified by
the reserved id `"main"`. RPC-only (no web UI).

## Deltas

- **tddy-service** (`proto/connection.proto`):
  - New `ConnectionService` RPCs: `StartTerminalSession`, `StopTerminalSession`,
    `ListTerminalSessions` (+ request/response messages, `TerminalSessionInfo{terminal_id, kind, pid}`).
    `kind` is `"claude-cli"` for the main tool and **`"bash"`** for started Bash tools.
    **(Updated: 2026-06-25 — kind label `"shell"` → `"bash"`.)**
  - `SessionTerminalInput.terminal_id` (field 4) and `StreamTerminalOutputRequest.terminal_id`
    (field 3) — optional; empty ⇒ `"main"` (back-compat).
- **tddy-daemon** (`claude_cli_session.rs`):
  - `MAIN_TERMINAL_ID = "main"`; `PtyHandle.terminal_id` / `PtyHandle.kind` (kind in
    {`"claude-cli"`, `"bash"`}).
  - Two-level registry `session_id → (terminal_id → PtyHandle)`; `get()` returns the `"main"` tool.
  - New manager API: `start_terminal` (spawns the **Bash tool** — `$SHELL` in the worktree, kind
    `"bash"`), `get_terminal`, `list_terminals`, `stop_terminal` (stubs in red).
  - `spawn_in_pty` generalized to a prebuilt `argv` + `terminal_id`/`kind`.
- **tddy-daemon** (`connection_service.rs`): `start_terminal_session` / `stop_terminal_session` /
  `list_terminal_sessions` handlers (stubs in red); existing 3 terminal handlers to route by
  `terminal_id` (green). The Bash tool's `$SHELL` is resolved at the RPC layer (`std::env::var
  ("SHELL")`, fallback `/bin/bash`) and passed to `start_terminal`.
- **tddy-daemon** (`session_deletion.rs`): expose pid SIGTERM/SIGKILL helper for `stop_terminal`.
- **tddy-tools** (`pty_relay.rs`): set `terminal_id: String::new()` on the two proto literals.

## Tests

`packages/tddy-daemon/tests/terminal_session_acceptance.rs` — 5 manager-level + 7 RPC-level tests
(start/stop/list/identity/`"main"` guard/auth/I/O routing). **(Updated: 2026-06-25 — started-tool
`kind` assertions `"shell"` → `"bash"`.)**

## Out-of-scope / pre-existing
- Future non-shell tools that take inputs (the API leaves room; only Bash — no inputs — is added now).
- **Pre-existing fix (not part of this feature):** `spawner.rs` `libc::initgroups` arg typed
  `gid as libc::c_int` (i32) but `libc 0.2.186` expects `gid_t` (u32) — the branch did not compile
  in this environment. Fixed to pass `gid` directly. Should land as its own small commit.

(tddy-service, tddy-daemon, tddy-tools, docs)
