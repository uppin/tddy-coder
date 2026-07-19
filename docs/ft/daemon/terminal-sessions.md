# Multiple Tools per Session (Bash tool)

> **Updated: 2026-06-29** — PTY tools (claude-cli, bash) are spawned via `PtyRuntime` into the shared
> `TaskRegistry` and appear in `tasks.TaskService.ListTasks`. `PtyRegistry` holds resize/control
> handles keyed by `task_id`. Terminal RPCs remain the external contract (compat layer).
>
> **Updated: 2026-06-26** — `StreamTerminalOutputRequest` gains `initial_cols`/`initial_rows`: daemon resizes PTY before replay, drains stale broadcast, triggers SIGWINCH — eliminates 220-col garbling on browser reconnect. `PtyHandle::send_input` strips OSC resize escapes. `kill_all()` added for clean daemon shutdown. Capture limit raised 64 KB → 512 KB.
>
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

## Terminal Control Ownership (Single-Screen Mutex)

> **Updated: 2026-06-26** — Extends the multi-tool RPC surface with a per-session control mutex
> so that exactly one screen can send input at a time.

### Summary

A tddy session's terminals — the main `claude` terminal plus any Bash tools — may be connected
from multiple browser tabs or clients simultaneously. Today each client can send input, causing
conflicting keystrokes. The control mutex ensures exactly **one screen is the controller** at any
given moment; all other connected screens are **observers** (they still receive output).

### Acceptance Criteria

#### Control Lease

10. `ClaimTerminalControl(session_id, screen_id, steal=false)` grants a `control_token` when the
    session is uncontrolled or already controlled by the same `screen_id`.
11. `ClaimTerminalControl(session_id, screen_id, steal=false)` is **denied** (returns
    `granted=false`, `current_holder_screen_id` set) when another `screen_id` holds the lease.
12. `ClaimTerminalControl(session_id, screen_id, steal=true)` always grants a new `control_token`,
    evicting the previous holder and emitting a `ControlChangeEvent` to all
    `WatchTerminalControl` subscribers for that session.
13. A session with no active lease is "uncontrolled": all input RPCs are accepted regardless of
    `control_token` (backwards-compatible with clients that do not yet send the field).

#### Input Enforcement

14. `SendTerminalInput` and `StreamSessionTerminalIO` with a `control_token` that does not match
    the current lease return `FAILED_PRECONDITION` ("terminal controlled by another screen").
15. The enforcement is transport-agnostic: the token is validated at the `ClaudeCliSessionManager`
    level, independent of whether the RPC arrived over HTTP Connect or LiveKit.

#### Watch / Notification

16. `WatchTerminalControl(session_id, control_token)` emits an immediate snapshot event
    (`TerminalControlEvent`) containing `holder_screen_id` and `you_are_controller` (true iff
    the subscriber's token matches the current lease).
17. When the lease changes (steal), all active `WatchTerminalControl` subscribers receive a new
    `TerminalControlEvent` reflecting the new holder.

#### Auth

18. `ClaimTerminalControl` and `WatchTerminalControl` require a valid `session_token`
    (→ GitHub user → OS user); an invalid token yields `UNAUTHENTICATED`.

### Non-goals (this iteration)

- No heartbeat-based auto-release when the controlling browser tab closes. A disconnected
  controller retains the lease; the next screen reclaims via `ClaimTerminalControl(steal=true)`.
- No persistence across daemon restart (the control registry is in-memory).
- The lease granularity is **per-session**, not per-terminal-id within a session.

---

## Session-scoped RPC routing & daemon-direct lifecycle

> The daemon is the **bootstrap/directory authority** and the **direct target for lifecycle
> control**; session-scoped `ConnectionService` methods for a LiveKit-backed (tddy-coder)
> session are served by the coder's own LiveKit participant. See
> [Session Participant RPC & Metadata](../coder/session-participant-rpc.md) for the coder side.

### Bootstrap / directory boundary

The daemon participant (`daemon-{instanceId}`) serves the calls the web makes **before or
without** an attached session participant:

- `StartSession`, `ConnectSession`, `ResumeSession`
- `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`,
  `ListProjectBranches`

### Session-scoped surface delegated

`ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, `ClaimTerminalControl`,
`WatchTerminalControl`, VNC, and screen-sharing for a LiveKit-backed session are served by
the coder's participant (`daemon-{instanceId}-{sessionId}`), not the daemon. The daemon still
serves these for **non-LiveKit** (claude-cli / cursor-cli / workspace) sessions where no coder
participant exists — that `ConnectionService` path is unchanged.

### `DeleteSession` / `SignalSession` — daemon-direct

The web calls `DeleteSession` / `SignalSession` **directly** on the daemon participant
(`daemon-{instanceId}`) with the caller's `session_token`; the coder is **not** on the path.
The daemon validates the token (GitHub user → OS user → session ownership) exactly as it does
for every other session RPC, performs process teardown / signalling, updates `.session.yaml`,
and returns the result. Daemon errors surface **verbatim** to the web caller. Serving these
daemon-direct keeps lifecycle control available even when the coder participant is
unresponsive, and lets the sessions list delete/signal an unattached row without a relay hop.

### Inspector data for sessions with no LiveKit participant

`SessionEntry` carries `bytes_in` / `bytes_out` / `last_data_received_at` fields. The daemon
populates these in `ListSessions` from the `GrpcSessionTerminal` traffic meter for
claude-cli / cursor-cli / workspace sessions it owns (live counters), and reports zero / empty
for tddy-coder sessions that have no LiveKit participant (stopped). The web inspector renders
these when no per-session live runtime exists (see
[Session Drawer Screen § Inspector I/O bytes + last-data-received](../web/session-drawer.md#inspector-io-bytes--last-data-received)).

### What stays the same

- Process lifecycle ownership stays with the daemon: it spawns and tears down the coder
  process and owns the `.session.yaml` record.
- `session_list_enrichment.rs` (`SessionListStatusDisplay`) keeps enriching `SessionEntry`
  from disk for inactive and directory-listed sessions.
- Auth model is unchanged: `session_token` → GitHub user → OS user → session ownership,
  validated at the daemon for every `DeleteSession` / `SignalSession` (always daemon-direct).
- claude-cli / cursor-cli / workspace sessions keep their existing daemon-served
  `ConnectionService` path.

---

## Non-goals (out of scope)

- Persisting tools across daemon restart (tools are in-memory, like the main terminal today).

> **Update (session-terminal-tabs):** two former non-goals are now delivered — **Web UI integration**
> (a terminal tab bar; see [Session Terminal Tabs](../web/session-terminal-tabs.md)) and **a LiveKit
> bridge for Bash tools** (coder/LiveKit sessions serve `terminal_id`-addressed terminal RPCs from
> their own participant; see [Session Participant RPC & Metadata](../coder/session-participant-rpc.md)).
> The PTY plumbing that both the daemon and the coder use now lives in the shared `tddy-pty` crate.
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
