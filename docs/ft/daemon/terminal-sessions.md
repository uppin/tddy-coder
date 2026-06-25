# Multiple Tools per Session (Bash tool)

> **Updated: 2026-06-25** — Reframed from "attachable terminal sessions" to **running multiple
> tools per session**. A session hosts identified tool instances; this release defines the **Bash**
> tool (a shell that takes no inputs and opens on the worktree). The RPC surface keeps its
> terminal-oriented names; each started terminal is a Bash tool instance.

## Summary

A tddy session can run **multiple tools**, not just the single `claude` CLI process it spawns
today. Each tool is a `portable-pty` process managed by `tddy-daemon`'s `ClaudeCliSessionManager`,
reusing the existing `PtyHandle` mechanic (PTY master + broadcast output + rolling capture-replay +
stdin channel + exit-monitor cleanup).

The first additional tool is **Bash** — a shell tool defined the same way the Claude CLI tool is,
but it takes **no inputs** and simply opens on the session's worktree directory.

Every tool carries a stable **terminal id**: the original `claude` tool is identified by the
reserved id `"main"` (kind `"claude-cli"`), and each started Bash tool gets a fresh id (kind
`"bash"`). Clients manage tools over RPC — start, stop, list — and address a specific tool's I/O by
id. This release is **RPC-only** (no web UI integration).

## User Story

As a client of `tddy-daemon`, I want to run extra tools (starting with Bash) inside a running
session and manage them over RPC, so I can run a shell in the session's worktree alongside the main
`claude` tool — with each tool individually identified and addressable.

## Acceptance Criteria

### Identification
1. Every tool is identified. The main `claude` tool is listed under the reserved id `"main"`;
   started Bash tools receive fresh, unique ids distinct from `"main"`.
2. `ListTerminalSessions(session_id)` returns one `TerminalSessionInfo{terminal_id, kind, pid}`
   per running tool of that session — `kind` is `"claude-cli"` for `"main"` and **`"bash"`** for
   started Bash tools. *(Updated: 2026-06-25 — kind `"shell"` → `"bash"`.)*

### Start / Stop
3. `StartTerminalSession(session_id)` starts a **Bash tool**: it spawns the user's login shell
   (`$SHELL`, fallback `/bin/bash`) in the session's worktree, with **no inputs**, and returns the
   new `terminal_id`. *(Updated: 2026-06-25 — the started tool is the defined Bash tool.)*
4. The Bash tool uses the same PTY mechanic as the main tool: stdin is writable, output is
   broadcast and captured for replay.
5. `StopTerminalSession(session_id, terminal_id)` terminates the tool's process and removes it from
   the registry; it no longer appears in `ListTerminalSessions`.
6. `StopTerminalSession` with `terminal_id = "main"` is rejected with `INVALID_ARGUMENT` — the main
   `claude` tool is managed through session lifecycle (`SignalSession` / `DeleteSession`), not this
   API.

### I/O addressed by terminal id
7. The terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`)
   accept an optional `terminal_id`. Empty `terminal_id` resolves to `"main"` (backward compatible
   with existing clients).
8. Output streaming and input delivery target the addressed tool; an unknown `terminal_id` yields
   `NOT_FOUND`.

### Auth
9. Every RPC validates `session_token` (→ GitHub user → OS user) exactly like the existing session
   RPCs; an invalid token yields `UNAUTHENTICATED`.

## Non-goals (out of scope)

- Web UI integration (deferred).
- Persisting tools across daemon restart (tools are in-memory, like the main terminal today).
- A LiveKit bridge for Bash tools (the main terminal's LiveKit path is unchanged).
- Tools that take inputs. The API leaves room for them, but the only tool added now is Bash, which
  takes no inputs. *(Added: 2026-06-25.)*
- Configurable Bash binary — the Bash tool is built-in (`$SHELL`, fallback `/bin/bash`); no config
  entry is required. *(Added: 2026-06-25.)*

## Architecture

### Registry & identity
`ClaudeCliSessionManager`'s registry becomes two-level: `session_id → (terminal_id → PtyHandle)`.
`PtyHandle` gains `terminal_id` and `kind` (kind in {`"claude-cli"`, `"bash"`}). The main `claude`
tool is inserted under the reserved id `MAIN_TERMINAL_ID = "main"`; `get(session_id)` remains a
convenience that returns the `"main"` tool so existing call sites are unchanged. The PTY
exit-monitor removes the tool by `(session_id, terminal_id)`.

### Tools & spawn
`spawn_in_pty` is generalized to accept a prebuilt `argv` plus `terminal_id`/`kind`, so both the
`claude` tool (`build_claude_argv`) and the Bash tool (`[shell_path]`) share the same I/O
threading, capture, and cleanup. The Bash tool resolves `$SHELL` (fallback `/bin/bash`) at the RPC
layer; the manager takes a resolved `shell_path` argument (no test-only branches) and labels the
instance kind `"bash"`. Bash takes no per-start inputs.

### Lifecycle
`StopTerminalSession` signals the tool's pid (reusing the SIGTERM/SIGKILL helper used by session
deletion) and removes the registry entry; this is idempotent with the exit-monitor's own removal.

### RPC surface (`ConnectionService`)
New methods `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` (terminal-named
for continuity; each operates on a tool instance); existing terminal I/O messages gain an optional
`terminal_id`. No new service — `ConnectionService` is already registered and wired in
`tddy-daemon`.
