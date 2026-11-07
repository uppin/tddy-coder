# Claude Code CLI Session

> **Updated: 2026-06-29** — Main `claude-cli` and auxiliary `bash` PTY tools spawn via `PtyRuntime`
> into the shared `TaskRegistry` (`kind` = `claude-cli` / `bash`). `ClaudeCliSessionManager` maps
> `terminal_id` → `task_id` and delegates control to `PtyRegistry`. Terminal RPCs unchanged.

> **Related:** [Cursor Agent CLI session](cursor-cli-session.md) — same daemon PTY manager and web terminal path with Cursor-specific hooks and argv.
> **Related:** [OpenCode CLI session](opencode-cli-session.md), [Muse Code CLI session](muse-cli-session.md) and [Warp CLI session](warp-cli-session.md) — the third, fourth and fifth CLI session types; all five are registered through one CLI harness descriptor.
>
> **Also runs as an agent tab:** the harness can be opened as an extra terminal inside another session, beside that session's main agent in its worktree — see [Session terminal tabs](../web/session-terminal-tabs.md).

## Summary

A new session type that spawns an interactive `claude` CLI process in a PTY within a git worktree, managed entirely by `tddy-daemon`. Users select **Claude Code CLI** as the session type from the web UI, choose a Claude model, and get direct terminal access — without any TDD workflow overhead. The daemon handles worktree creation, PTY lifecycle, session persistence, and resume if the process exits.

This is architecturally distinct from the existing `CodingBackend`-based sessions (where `tddy-coder` orchestrates workflow tasks and invokes `claude` per-task). A Claude Code CLI session is a **raw persistent shell** — the user drives `claude` directly.

## User Story

As a developer, I want to start a raw Claude Code CLI session from the tddy web UI so I can interact with `claude` interactively inside a managed worktree — with the same session persistence and resume capabilities as regular tddy-coder sessions, but without any TDD workflow scaffolding.

## Acceptance Criteria

### Start session
1. The start-session form shows a **Session type** toggle/selector alongside the existing **Tool** dropdown. When **Claude Code CLI** is selected:
   - The **Workflow recipe** dropdown is hidden.
   - A **Model** dropdown replaces the **Backend** dropdown, populated from `ListAgents` (filtered to `claude-cli`-capable agents).
   - The **Tool** dropdown is hidden (tool binary is always `claude`).
2. Clicking **Start New Session** issues `StartSession` with `session_type = "claude-cli"` and the chosen `model`.
3. The daemon creates a git worktree at `<repo>/.worktrees/claude-cli-<short-session-id>/` on branch `claude-cli/<short-session-id>`.
4. The daemon spawns `claude --model <model>` inside the worktree in a PTY.
5. `StartSessionResponse` returns `session_id`; no LiveKit credentials (empty strings).

### Terminal access
6. The web UI opens a `GhosttyTerminalSession` component connected to `StreamSessionTerminalIO` (gRPC bidirectional stream on `terminal_session.TerminalSessionService`).
7. All keyboard input and output are relayed in real time. Terminal resize events update the PTY.
8. No LiveKit connection is made for Claude Code CLI sessions.

### Session persistence
9. The session directory `~/.tddy/sessions/<session-id>/` is created with `.session.yaml` containing:
   - `session_type: claude-cli`
   - `model: <model-id>`
   - `sandbox: true` (optional; platform jail spawn when set — macOS Seatbelt / Linux cgroups)
   - `repo_path: <worktree-absolute-path>`
   - `pid: <claude-process-pid>`
   - `status: active`
10. The session appears in `ListSessions` with `workflow_goal`, `workflow_state`, and `elapsed_display` showing `—` (em dash), and `model` showing the selected model.
11. The `agent` column shows `claude-cli`.

### Resume after process exit
12. When the `claude` process exits (for any reason), the daemon marks the session `status: inactive` in `.session.yaml` (PID cleared).
13. The session row shows **Resume** in the session table (same as any inactive session).
14. Clicking **Resume** issues `ResumeSession`. The daemon:
    - Reads `session_type`, `model`, and `repo_path` from `.session.yaml`.
    - Verifies the worktree still exists (errors with a clear message if not).
    - Spawns a new `claude --model <model>` PTY inside the existing worktree.
    - Updates `.session.yaml` with the new PID and `status: active`.
15. Terminal access resumes immediately via the same `StreamSessionTerminalIO` stream pattern.
16. What the agent printed outlives the process: the session's `main` terminal is recorded to
    `~/.tddy/sessions/<session-id>/terminals/main.capture` while it runs (every 30 s) and at exit, on
    start and on resume, and a stopped session's `GetTerminalHistory(main)` is served from it — see
    [Terminal sessions § History of a stopped agent terminal](terminal-sessions.md#history-of-a-stopped-agent-terminal).

### Connect (reattach)
16. When the `claude` process is still running and the web client disconnects/reconnects, `ConnectSession` finds the existing PTY and reopens the stream — no new process is spawned.

### Signal and delete
17. `SignalSession` sends the signal to the `claude` PTY process.
18. `DeleteSession` sends SIGTERM, waits for exit, removes the session directory and worktree.

## Sandbox mode (`StartSessionRequest.sandbox = true`)

`StartSession` with `session_type = "claude-cli"` and **`sandbox = true`** runs `claude` inside a
**platform jail** on the same host — macOS **Seatbelt** or Linux **rootless cgroups + namespaces**.
The daemon still creates a git worktree and executes tools against it via `tool_engine::execute_tool`;
the sandboxed agent reaches the codebase only through `mcp__tddy-tools__*` tool calls relayed over a
host-initiated gRPC **`SessionChannel`**. No LiveKit credentials are returned. The flag is set from the
tddy-web new-session form or `tddy-tools pty-relay --sandbox`.

| Aspect | Non-sandbox claude-cli | Sandboxed claude-cli |
|--------|------------------------|----------------------|
| `claude` spawn | Direct PTY in worktree | jail (`sandbox-exec` on macOS / userns+cgroups on Linux) → `tddy-sandbox-runner` → PTY |
| Filesystem | Full host worktree | Read-only context dir in jail; writes via MCP tools on host |
| Network from agent | Host network | No direct egress (Seatbelt `deny network*` / Linux network namespace); HTTPS tunneled over `SessionChannel` to a host relay (TLS end-to-end), only to the hosts `claude.yaml` declares plus a provider route's base-URL host |
| Control channel | — | loopback TCP (macOS) / AF_UNIX (Linux, survives the network namespace) |
| `HOME` | The host user's | `<tddyhome>/homes/claude`, persistent across sessions |
| Resume / delete | PTY respawn / SIGTERM | Stop sandbox child + relaunch runner; same worktree teardown |
| Unsupported host | Supported | `failed_precondition` (no userns / no writable cgroup v2 subtree / other OS) — no fallback |

`.session.yaml` records `sandbox: true`. The production daemon runs as a root systemd service, where
Linux's unprivileged-userns restriction does not apply. See
[connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions),
[tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md), and
[remote-codebase-mode.md](remote-codebase-mode.md).

### Sandboxed start (macOS)

19. `StartSession` with `session_type = "claude-cli"` and **`sandbox = true`** on macOS spawns
    `tddy-tools sandbox-runner` inside Seatbelt; the daemon dials the in-jail **`SessionChannel`**
    for terminal I/O, MCP tool exec, and LLM egress relay.
20. Sandboxed sessions return empty LiveKit credentials; terminal access uses the same
    `StreamTerminalOutput` / `SendTerminalInput` RPCs as non-sandbox claude-cli, resolved out of the
    sandbox registry by the daemon's composite terminal store and served by the same bridge — so a
    jailed terminal replays, anchors and pages like every other one.
21. The agent reads a read-only context dir in the jail; codebase mutations flow through
    `mcp__tddy-tools__*` tool calls executed on the host worktree.
22. On non-macOS, `sandbox = true` returns `failed_precondition` (no fallback).

### The jail from the `claude` tool definition

A sandboxed start and its relaunch launch Claude through the **shared jail launcher**, from the same
`claude.yaml` the unsandboxed launch renders ([§ Launch](#launch-from-the-claude-tool-definition)).
The definition's generic jail sections decide the jail; nothing on the daemon path hard-codes Claude:

```yaml
home: { kind: persistent, key: claude }            # <tddyhome>/homes/claude, mounted read-write
files:
  - { kind: seed-once, from_host: .claude/.credentials.json, to: .claude/.credentials.json }
  - { kind: mirror, from_host: .local/bin/claude, to: .local/bin/claude }
  - { kind: mirror, from_host: .local/share/claude/versions, to: .local/share/claude/versions }
  - { kind: overlay, env: CLAUDE_CODE_TMPDIR, value: "{tmp}" }
  - { kind: overlay, env: CLAUDE_TMPDIR, value: "{tmp}" }
secrets:
  - env: CLAUDE_CODE_OAUTH_TOKEN
    source: { kind: host-env, var: CLAUDE_CODE_OAUTH_TOKEN }
    when: unrouted                                  # a provider route brings its own credentials
policy:
  jit: true
  fork: true
  sysctl_read: true
  exec_paths: [/usr/bin, /usr/libexec, /bin, /sbin, /System, /Library, /Applications, /opt/homebrew, /opt/local]
network:
  egress:
    hosts: [api.anthropic.com, statsig.anthropic.com, sentry.io, console.anthropic.com, claude.ai]
  oauth_inbound: true
```

The section vocabulary is the [sandbox builder's](../coder/sandbox-builder.md#jail-sections), shared
by every tool. What Claude sees in the jail is an ordinary environment:

- **`HOME`** is `<tddyhome>/homes/claude`, shared by every sandboxed Claude session and kept across
  them, so refreshed OAuth tokens, history and settings survive. The host's `.credentials.json` is
  copied in only when the home lacks one; Claude's own install (`~/.local/bin/claude`, its
  `versions/`) is linked in where its startup self-check looks for it.
- **`PATH`** is a jail-local directory of the declared tools (`claude`, `tddy-tools`) ahead of the
  system directories. `CLAUDE_CODE_TMPDIR` / `CLAUDE_TMPDIR` point into the jail's tmp.
- **`HTTPS_PROXY`** (both casings, loopback bypassed) points at the runner's egress shim.
- **No `TDDY_*` variable.** The `tddy-tools --mcp` server Claude registers carries what it needs —
  `TDDY_SANDBOX_TOOL_IPC`, `TDDY_SANDBOX_SESSION_ID`, its log and accounting files, and the daemon's
  `TDDY_*` additions for the session — in its own `env` block of the MCP config.
- **Argv** is `claude.yaml`'s template: `--model`, `--session-id` (or `--resume` on a relaunch), the
  permission mode (blank is `auto`) or `--dangerously-skip-permissions`, the client's pass-through
  args and first prompt, then the slot flags — `--append-system-prompt-file`, the roster's
  `--allowedTools` / `--disallowedTools`, `--permission-prompt-tool`, and `--mcp-config` with the
  config in the jail's tmp. The MCP slot is rendered **without** `--strict-mcp-config`, so a jailed
  agent loads the project's own `.mcp.json` beside `tddy-tools`; whether it should is open
  ([todo](../../dev/todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)).

**Network.** The jail reaches the declared hosts through the shim and is refused every other one,
both at the shim (`403`) and at the host relay's `TunnelOpen`. A session on a provider route also
reaches the route's base-URL host, carries no `CLAUDE_CODE_OAUTH_TOKEN`, and opens no OAuth inbound
port; an unrouted session gets the host's token (when the daemon's environment has one) through the
`TDDY_SECRET_*` channel, and the loopback OAuth callback.

**The runner is told what to exec.** The daemon writes the rendered argv, env and slot files to
`<session>/sandbox/exec-plan.json` and spawns `tddy-sandbox-runner --exec-plan` on it. No
Claude-specific flag (`--claude-binary`, `--claude-arg`, `--permission-mode`, `--resume`,
`--append-system-prompt-file`) reaches the runner from the daemon.

**The home's location.** The daemon's jailed Claude home is always `<tddyhome>/homes/claude`:
the spec's `home.key` (`claude`) and `TDDY_SANDBOX_CLAUDE_HOME` do not move it (the
standalone `tddy-sandbox-app` still uses its own home). An operator who wants a directory of their
own declares `home: { kind: path, path: <absolute dir> }` in a `<tddyhome>/tools/claude.yaml`
override: that directory is mounted read-write, and a relative path is refused when the tool file
loads (`#agent-tools` 19/19). The first sandboxed start finding
`<tddyhome>/sandbox-claude-home` renames it to `homes/claude` once, credentials and history kept. A
start that finds both refuses with `FAILED_PRECONDITION` naming both, and touches neither. A former daemon.yaml
`claude_home_dir` is not migrated: the new home is seeded from the host's
`~/.claude/.credentials.json`.

**Role.** The launcher records the session's `session_role` before the spawn, so a project-home main
set up in a jail is listed with `session_role: project-home`. A jailed main is launched with an
ordinary sandboxed session's tool flags, not the main's role wiring
([todo](../../dev/todo/2026-10-24-sandboxed-main-needs-session-role.md)).

**Requirements.** A sandboxed start on a host without coding agents installed is
`FAILED_PRECONDITION`; the daemon always installs them. The jail is exercised by tests on macOS
Seatbelt only: no CI host runs a Linux jail
([todo](../../dev/todo/2026-11-02-ci-runs-no-linux-jail.md)).

## tmux mode (`StartSessionRequest.run_in_tmux = true`)

A claude-cli session created with **Run the agent in tmux** (`run_in_tmux = true`) runs Claude inside a
**detached tmux session on the session's host, as the session's OS user**, in the session's worktree.
The session's main terminal is a tmux client attached to it, so the agent is an ordinary tmux session
on the host: it shows in `tmux ls` and can be attached from any terminal over SSH.

- **Name.** `tddy-<full session id>` (a `:` or `.` becomes `_`, as tmux itself substitutes). The whole
  id, never a prefix: the leading characters of a UUIDv7 are its millisecond timestamp, so a prefix
  repeats across sessions started close together. The name is recorded as `tmux_session` in
  `.session.yaml`, and it is derivable from the id alone.
- **Launch.** `tmux new-session -d -s <name> -c <worktree> -- /bin/sh -c '. "$1"; rm -f -- "$1";
  shift; exec "$@"' sh <env file> <claude argv>` runs as the session's OS user — the argv is the one a
  PTY start builds. A pane inherits the tmux *server's* environment, not a client's, so the pane's
  environment — the target user's `HOME` and configured `PATH`, then the session's on top (toolcall
  socket, hooks, provider route, renderer settings) — is handed over in a **file**, never on an argv:
  the daemon writes it as `export K='v'` lines (shell-quoted) to a fresh file under
  `<session dir>/tmux-env/`, created with mode `0600` and owned by the session's OS user (the directory
  is `0700`, the user's too), and the pane sources it, deletes it and `exec`s Claude. No value —
  the provider token included — appears on any process's argv. A start that fails removes the file
  itself. Then the main terminal runs `tmux -T clipboard attach-session -t =<name>` as that user.
- **pid.** The recorded `pid` is tmux's `#{pane_pid}` — the Claude process — not the client's. Liveness,
  **SignalSession** and the delete's process teardown act on the agent.
- **Delete** kills the tmux session (as the session's OS user) **before** anything else of the session
  is torn down, then terminates the recorded pane pid (SIGTERM, then SIGKILL), so an agent that ignores
  SIGHUP is gone too. A recorded tmux session that cannot be killed refuses the delete with the session
  intact. When `.session.yaml` cannot be read, the delete kills `tddy-<session id>` anyway — the name
  is authoritative — treating "no such session" as fine, and a tmux that cannot be run at all is
  logged without blocking the delete of the broken session.
- **The agent outlives its client and its daemon.** The main terminal is only a tmux *client*: daemon
  shutdown stops it, which **detaches** it, and the agent and its tmux session keep running (a tab's
  tmux session was never touched either).
- **Client exit vs agent exit.** When the main terminal's client ends, the daemon asks tmux whether the
  recorded tmux session still lives (`has-session`, as the session's OS user, off the executor).
  - **Alive** — only the client ended: the terminal entry is dropped and its recording finished, but
    the session's managed workflow (its toolcall socket) and its LiveKit exposure stay, since the agent
    still uses them.
  - **Gone** — the agent ended: the ordinary main-exit cleanup drops both.
  - A probe that cannot finish tears nothing down.
- **Detach watcher.** A detached agent is watched: every **10 s** the daemon checks whether its tmux
  session still lives, and runs the agent-ended cleanup (toolcall socket, exposure) once it is gone. A
  re-attach stops the watcher; daemon shutdown stops every watcher and starts none.
- **Re-attach.** A session whose tmux session is alive and has no main terminal registered gets one
  re-attached — the agent is **never relaunched**, and the recorded `pid` is re-read from the pane:
  - on **Resume**. A daemon that still serves the session's managed workflow (a client that detached
    from this daemon) keeps it, so the socket its session pointer and tool server name stays bound; a restarted daemon, which has
    none, prepares one bound to the same `tddy-wf-<id>.sock` path. A main already attached to that tmux
    session is returned as it is, so a second Resume adds no second client;
  - on the **first stream, input or history read** of its `main` terminal — the session reads active
    (its pane pid lives), so the web never offers Resume. The re-attach reads `.session.yaml`, probes
    tmux as the requester's OS user, resolves the agent from the recorded session type among the host's
    installed coding agents, attaches, and registers the main recording; it is single-flight per
    session, so concurrent first accesses attach one client. A record without a session type is
    skipped with a warning (it is never re-attached as claude-cli), and a host without coding agents
    re-attaches nothing.
- **Resume** of a tmux-run session whose tmux session is gone kills any leftover tmux session of that
  name, then starts a fresh one running Claude's resume argv (`--resume <id>`).
- **A failed attach** kills the tmux session that start created: nothing records it, so nothing could
  stop it later.

`run_in_tmux` is refused with `INVALID_ARGUMENT` naming the conflict, before routing and before anything
is created, together with:

| With | Why |
|---|---|
| `sandbox` | the agent runs in its own jail, not in a tmux session on the host |
| `sandboxed_codebase` | that placement withdraws the agent's native tools and jails the code — there is no host agent to run in tmux |
| `codebase_daemon_instance_id` (split placement) | the worktree is on another daemon, and the tmux session runs in a worktree on this host |

[Cursor CLI](cursor-cli-session.md#tmux-mode), [OpenCode](opencode-cli-session.md#tmux-mode),
[Muse Code](muse-cli-session.md#tmux-mode) and [Warp](warp-cli-session.md#tmux-mode) sessions honour
`run_in_tmux` with the same behaviour and the same refusals; tool and workspace sessions ignore it. A
harness with an exit hook (Warp's resume token) keeps its pane after the agent exits and reads the
hook's output from it — see [Warp § tmux mode](warp-cli-session.md#tmux-mode). The agent-facing
StartSession tool offers it as a boolean
beside `sandbox`. On a supervised install a tmux-run session for another OS user is refused with
`CannotBecomeUser` rather than run as the daemon user.

Known limitations:

- `SessionMetadata` is `deny_unknown_fields`, so a daemon older than tmux mode cannot read a tmux-run
  session's record (downgrades are not supported).
- Nothing records the agent's output while no client is attached: the main recording ends at the
  client's exit, so an agent that ends while detached leaves the history as of that moment — see
  [`docs/dev/todo/2026-11-03-detached-tmux-agent-output-is-not-recorded.md`](../../dev/todo/2026-11-03-detached-tmux-agent-output-is-not-recorded.md).
- A re-attach does not re-register the LiveKit exposure: a client's exit on the same daemon keeps it,
  and a restarted daemon has nothing to rebuild it from (`stack_parent` is not persisted).
- The tmux control commands (`new-session`, `kill-session`, `display-message`) drop privilege through
  the kernel's `run_output_as_user`, the client through the PTY runtime — the same split as tmux tabs,
  see [`docs/dev/todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md`](../../dev/todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md).

## Running on a provider account

A claude-cli session can run Claude Code on one of the operator's
[provider accounts](provider-accounts.md) instead of Anthropic. The session's Claude Code then talks
to that account's Anthropic Messages endpoint with that account's key, and every model it may choose
is pinned to the session's model, so no request reaches Anthropic.

### Choosing the account

`StartSessionRequest` carries the session-level choice:

| Field | Meaning |
|---|---|
| `provider_account_id = 41` | run on this account (an id `ConnectProviderAccount` gave). Empty = no choice at this level |
| `provider_built_in = 42` | explicitly the vendor ("Anthropic (built in)"), overriding an agent definition's or the person's default account |

The two are exclusive; a request that sends both runs on the named account, never on the vendor the
operator did not ask for. The account a session runs on is chosen by precedence — **session over
agent definition over the person's default** — and an explicit built-in choice at a higher level
overrides an account at a lower one. The session level comes from the start request and the
default level from the owner's vault ([a person's default per harness](provider-accounts.md#a-persons-default-per-harness));
nothing fills the agent-definition level yet. A start that names nothing reads the owner's default
for `claude-cli` and refuses while the vault is locked; an explicit choice never reads it. A start
with no choice and no default runs exactly as a session on Anthropic always has (host
`.credentials.json`, OAuth token).

The web's New-session form offers the choice as **Provider** — see
[tool-session model selection § Provider](../web/tool-session-model-selection.md#provider).

### What the session is launched with

An unsandboxed claude-cli start resolves the account **before anything is created** — no session
directory, worktree or process exists when it refuses. It reads the account from the session owner's
vault exactly as it reads their GitHub identity, then launches `claude --model <model>` in the PTY
with, added to the session's environment:

| Variable | Value |
|---|---|
| `ANTHROPIC_BASE_URL` | the account's Anthropic Messages root |
| `ANTHROPIC_AUTH_TOKEN` | the account's key |
| `ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL` | the session's model |

The key reaches only the agent child's environment: it is never on argv and never logged. Unlike the
GitHub token there is no per-call socket Claude Code could ask instead, so the environment is where it
reads its key.

**No fallback.** A start that chose an account never runs on Anthropic instead. It is refused with:

| Situation | Status |
|---|---|
| The owner's vault is locked or uninitialized, or no vaults are configured | `FAILED_PRECONDITION` |
| The vault holds no provider account with that id | `NOT_FOUND` |
| The account's endpoint does not serve Anthropic Messages (Baseten, Modal, an OpenAI-compatible endpoint) | `INVALID_ARGUMENT`, naming the protocol Claude Code needs |
| An account was chosen with an empty model | `INVALID_ARGUMENT` — choose a model the account serves |
| The harness cannot be routed in this build | `UNIMPLEMENTED` |

A **sandboxed** claude-cli start (`sandbox = true`) that chooses an account runs on it inside the
jail. The account is resolved before the session directory is created, with the same refusals as
above. The jail receives the route in two parts:

- the non-secret variables (`ANTHROPIC_BASE_URL`, the model variables) in its explicit environment;
- the key (`ANTHROPIC_AUTH_TOKEN`) through the jail's secret-file channel (`TDDY_SECRET_<NAME>`). The
  runner sets it on the agent child only and unlinks the file, so the key never appears in the jail's
  environment map or on argv.

The host's `CLAUDE_CODE_OAUTH_TOKEN` is not injected beside a route (`claude.yaml` guards it
`when: unrouted`), the jail's OAuth inbound port is not opened, and the route's base-URL host is added
to the jail's egress hosts. A resume relaunches the jail on the account the start recorded, re-resolved as below.
A sandboxed start that chooses nothing is unchanged.

A **child** session a pr-stack orchestrator spawns, and a **conversation** a grill-me orchestrator
spawns, run the orchestrator's own agent (a claude-cli orchestrator's child is a claude-cli session,
a cursor-cli one's a cursor-cli session) on the vendor: the orchestrator's account is not inherited.
A host without coding agents, or an orchestrator that records no CLI session type, refuses the child
`FAILED_PRECONDITION`.

### Persistence and resume

A session started on an account records it in **`provider-account.yaml`** beside `.session.yaml` —
the account id and the model, never the key. A session on its vendor writes no file.

Resume re-reads that record and resolves the account against the owner's vault **as it stands at the
resume**: a rotated key is what the resumed session runs on, and an account removed since the start —
or a vault locked since — refuses the resume with the same statuses a start gets, rather than running
it on Anthropic. A session with no record resumes on the vendor.

## Non-goals (out of scope)

- Web UI support for sandboxed sessions (service-level only; same as remote mode).
- TDD/bugfix workflow integration — Claude Code CLI sessions have no recipe.
- Telegram elicitation via the structured `PresenterObserver` / `ModeChanged` pipeline (that path is for tddy-coder workflow sessions only). Claude Code CLI elicitation alerts are delivered via the `ReportSessionStatus` / `WaitingForInput` path — see [telegram-notifications.md § Claude Code CLI session activity alerts](telegram-notifications.md#claude-code-cli-session-activity-alerts).
- Model switching after session start.
- Session chaining (`previous_session_id`) for Claude Code CLI sessions.
- LiveKit presence for Claude Code CLI session processes (no `--livekit-*` args passed).
- Switching a session's provider account after its start.

## Architecture

### Session type discriminant

`StartSessionRequest` gains a `session_type` string field (`"tool"` default, `"claude-cli"` new). The daemon branches on this field in `connection_service.rs`. Existing sessions are unaffected (absent or empty `session_type` = `"tool"`).

### `SessionMetadata` additions

```yaml
session_type: claude-cli    # optional; absent/empty = "tool"
model: opus                 # optional; stored for resume
sandbox: true               # optional; darwin Seatbelt jail (macOS only)
tmux_session: tddy-<id>     # optional; the tmux session a tmux-run agent runs in (pid is its pane's)
```

`repo_path` reuses the existing field to store the **worktree absolute path** (not the main repo path, unlike tool sessions which may store the project repo root).

### Terminal streaming: `StreamSessionTerminalIO`

On `terminal_session.TerminalSessionService`
(`packages/tddy-terminal-rpc/proto/terminal_session.proto`):

```proto
rpc StreamSessionTerminalIO(stream SessionTerminalInput) returns (stream SessionTerminalOutput);

message SessionTerminalInput {
  string session_token = 1;  // sent on first message; ignored on subsequent
  string session_id    = 2;  // sent on first message; ignored on subsequent
  bytes  data          = 3;
  // plus terminal_id, control_token and input_offset — see terminal-sessions.md
}
```

The daemon maintains a per-session `PtyHandle` in an in-memory registry keyed by
`(session_id, terminal_id)` and exposes it through the `TerminalSessionStore` port, so the stream
itself — the write task, the read task, the replay and the offset arithmetic — is
[`tddy-terminal-rpc`'s bridge](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md).
Multiple simultaneous web clients can attach to the same PTY (shared read broadcast).

`terminal.TerminalService` / `TerminalServiceVirtualTui` is a separate surface and is not used for
Claude CLI sessions.

### PTY management in daemon

New `ClaudeCliSessionManager` in `tddy-daemon`:

- **Start**: creates worktree, opens PTY (`openpty`/`posix_openpt`), `fork`/`exec` `claude --model <model>` inside the worktree, stores `PtyHandle` (master fd + PID) in the registry.
- **Monitor**: a background task watches the PID via `waitpid`; on exit, marks the session inactive in `.session.yaml` and removes the PTY from the registry.
- **Resume**: opens a fresh PTY, re-exec's `claude --model <model>` in the existing worktree, updates the registry.
- **Connect**: looks up existing `PtyHandle` — no re-exec if the process is alive.

### Launch from the `claude` tool definition

An unsandboxed `claude-cli` start and resume launch the agent from its **tool definition**,
`claude.yaml`, through one spec-driven PTY launcher. Nothing in that launch path hard-codes Claude.
The definition is embedded from `packages/tddy-coding-agent-specs/specs/claude.yaml`, and
`<tddyhome>/tools/claude.yaml` replaces it **whole**:

```yaml
id: claude
binary: { command: claude, env: TDDY_CLAUDE_BINARY }
deps: [tddy-tools]                       # the MCP relay and hook binary, itself a built-in tool
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
    - { if: resume, then: ["--resume", "{session_id}"], else: ["--session-id", "{session_id}"] }
    - if: dangerously_skip_permissions
      then: ["--dangerously-skip-permissions"]
      else:
        - { if: permission_mode, then: ["--permission-mode", "{permission_mode}"], else: ["--permission-mode", "auto"] }
    - { splice: slots }                   # the slot flags, before the prompt
    - { if: prompt, then: ["{prompt}"] }
  env: {}
extensions:
  coding_agent:
    mcp: { kind: file, flag: --mcp-config, format: claude-mcp-json, strict_flag: --strict-mcp-config }
    tool_allow: { flag: --allowedTools }
    tool_deny: { flag: --disallowedTools }
    permission_prompt: { flag: --permission-prompt-tool }
    instructions: { files: [CLAUDE.md, AGENTS.md], append_system_prompt: { flag: --append-system-prompt-file } }
    hooks: { adapter: claude-settings-local }
    conversation: { kind: daemon_session_id }   # --session-id on a start, --resume on a resume
    capabilities: [hooks, managed_workflow, ask_user_question, split_host, project_home_main, requires_model, branch_name_required, operator_notify]
    models: { default: opus, curated: [opus, sonnet, haiku, …] }
    context_globs: [AGENTS.md, ".agents/**", CLAUDE.md, ".claude/**", .mcp.json]
```

- **Binary.** A `<tddyhome>/tools/claude.yaml` override's `binary.command` replaces the spec's
  binary; `TDDY_CLAUDE_BINARY` applies, then the host auto-resolution (`~/.local/bin`, then
  `PATH` without wrapper shims), then the spec's `PATH` search. `tddy-tools` resolves through
  `TDDY_TOOLS_BINARY`, then `CARGO_BIN_EXE_tddy-tools` or the daemon's sibling, then `PATH`; its
  `binary.command` is overridden by `<tddyhome>/tools/tddy-tools.yaml`. daemon.yaml `claude_cli:`
  (`binary_path`, `tddy_tools_path`) is refused at start-up.
- **Argv.** The template renders the session's model, id, permission mode (default `auto`), resume
  flag and first prompt. A blank prompt is no prompt. The slot flags sit before the positional
  prompt: `--mcp-config` is variadic, so the `strict_flag` that follows the config path keeps the
  prompt from being swallowed.
- **Slots.** The `coding_agent` extension declares the tddy integration points, and the launch fills
  them:
  - The hooks adapter writes `<worktree>/.claude/settings.local.json`, the six lifecycle hooks that
    call `tddy-tools session-hook`.
  - A session's appended system prompt goes through `--append-system-prompt-file`.
  - A project-home main's pre-approved tools go through `--allowedTools`. Its withdrawn native
    filesystem and shell tools go through `--disallowedTools`.
  - The main's `tddy-tools --mcp` server is registered as `<session dir>/claude-mcp-config.json` and
    loaded with `--mcp-config … --strict-mcp-config`.
- **Not slotted.** A main's role prompt and a split session's wiring arrive as rendered flags after
  the slot flags. The slot takes one prompt file, and a managed recipe's prompt can hold it.
- **Refusals.** The launcher refuses a spec whose `run.transport` is not `pty`, naming the spec. A
  tool file under `<tddyhome>/tools` that does not load fails daemon start, naming the file. That
  covers an unknown field, an unknown slot kind, and an `extensions:` key no owner claims.
- **Hooks are written at the spawn.** A hooks file that cannot be written fails the start.
- **Resume** renders `--resume <id>` from the same template. It does not rewrite the hooks file or
  replay the first prompt, and it runs with the default permission mode.

There is no other launch: a host without coding agents refuses every claude-cli start, resume and
child `FAILED_PRECONDITION`, and re-attaches nothing after a restart (logged). Only the
session-lifecycle suites build such a host; the daemon always installs coding agents. The sandboxed
`claude-cli` path renders the same definition through the jail launcher
([§ The jail](#the-jail-from-the-claude-tool-definition)).

### The generic CLI spec launch serves Claude's needs

The generic CLI launch — the one launch every CLI agent takes, `claude-cli` and `cursor-cli`
included, `<id>-cli` for a `<tddyhome>/tools/<id>.yaml` — gives an agent what `claude.yaml` declares, each feature keyed on a
spec slot or capability and never on the agent's id. A copy of `claude.yaml` dropped in under another
id therefore starts, in tmux, as a project's main and on an ssh checkout with the argv, files and
environment the `claude-cli` launch produces:

- **Conversation.** `coding_agent.conversation: { kind: daemon_session_id }` binds the daemon's
  session id: `--session-id <id>` on a start, `--resume <id>` with the recorded model on a resume.
- **Managed workflow.** `managed_workflow` admits a managed-codebase start; the recipe's prompt is
  appended through `--append-system-prompt-file` and the workflow is attached once the agent runs. An
  agent without it is refused `INVALID_ARGUMENT`, naming the agent and `managed_workflow`.
- **Hooks and notify.** `hooks` writes the hooks adapter's settings file; `operator_notify` appends the
  notify guidance and puts `tddy-tools notify`'s session into the session pointer
  ([§ Notifying the operator](#notifying-the-operator)).
- **Project-home main.** `project_home_main` admits the role (any other agent is refused, naming the
  agents that can run main); the role is recorded before the spawn and the main's tools are restricted
  through the `tool_allow` / `tool_deny` slots and its `tddy-tools --mcp` registration.
- **tmux.** The spec's rendered argv runs in the session's own tmux session, on a start and a resume.
- **ssh checkout.** On an ssh checkout the slot files are written on the target over ssh (bounded by
  the 60 s slot-write deadline; a target that never answers refuses the start `DEADLINE_EXCEEDED`
  naming the host), never at that path on this host.
- **No `TDDY_*` variable.** The agent process sees no `TDDY_*` variable: a host PTY runs under
  `env -u` for each the daemon holds, and a tmux pane unsets every one it inherited from the tmux
  server before it execs the agent. A managed workflow's or a main's `TDDY_*` pair is carried by the
  agent's tool-server registration (`mcp` env), never the process.
- **Session pointer.** So a `tddy-tools submit`, `transition`, `ask` or `notify` the agent runs from
  its own shell still reaches the session, the launch writes `<worktree>/.tddy/session.json` at every
  start and resume ([tddy-tools session resolution](../../../packages/tddy-tools/docs/session-resolution.md)):
  the managed workflow's socket, session and repo directories, the operator-notify session, and its
  owner — the daemon's pid and host. It is owner-only (`0600`), added to the repository's
  `info/exclude` when the worktree is in a git checkout, replaced on resume, and removed when the
  agent exits (in tmux only when the launch has an exit hook, which keeps the pane; without one it is
  replaced at the next resume and removed at the delete) and when the session is deleted. On an ssh
  checkout the same holds on the target: one ssh command, bounded like the slot write, adds the
  pointer to the target checkout's `info/exclude` and writes it owner-only, and the removals are a
  bounded ssh command each. A `tddy-tools` on the daemon's host refuses a pointer whose owner has
  exited or whose socket is gone, naming the pointer; one on the ssh target does not check an owner on
  another host. The remote agent is not yet recorded as an ssh pointer's owner
  ([todo](../../dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md)). A jailed session
  writes none: its shell relay carries the session, and a pointer copied into the jail would name a
  host socket the jail cannot reach.
- **Jail.** The generic jailed start and resume bind the same conversation, permission mode and
  session id, and carry a managed workflow for a spec declaring `managed_workflow`.
- **Branch intent.** A `new_branch_from_base` start naming no branch is refused exactly when the
  spec declares `branch_name_required` (`claude` does, `cursor` does not).

No agent id is compared in the launch: a source scan
(`tddy-session-lifecycle/tests/no_agent_name_gates.rs`) pins that no `"claude"` / `"cursor"` /
`"claude-cli"` / `"cursor-cli"` literal is dispatched on in `tddy-agent-launch`.

### Worktree naming

Branch: `claude-cli/<first-8-chars-of-session-uuid>`
Worktree path: `<main-repo-root>/.worktrees/claude-cli-<first-8-chars>/`

Uses existing `create_worktree_with_retry()` from `tddy-core`.

### Web: session type selector

`ConnectionScreen` form gains a **Session type** radio/tab with two options:

| Option | Shows | Hides |
|--------|-------|-------|
| **Managed session** (default) | Tool, Backend, Recipe | — |
| **Claude Code CLI** | Model dropdown | Tool, Backend, Recipe |

The Model dropdown is populated from `ListAgentModels(agent: "claude-cli")` and preselects the
response's `default_model`. The web holds no model list of its own; the catalog lives in
`tddy_core::backend::claude_cli_models()` — versionless aliases first, version-pinned ids after:
- `opus` → "Claude Opus (latest)" ← default
- `sonnet` → "Claude Sonnet (latest)"
- `haiku` → "Claude Haiku (latest)"
- `claude-opus-5` → "Claude Opus 5 (pinned)"
- `claude-sonnet-5` → "Claude Sonnet 5 (pinned)"
- `claude-haiku-4-5-20251001` → "Claude Haiku 4.5 (pinned)"

### Web: terminal component

The `GhosttyTerminalSession` component wraps `GhosttyTerminal` and connects `onData`/`onResize` to a gRPC `StreamSessionTerminalIO` bidi stream on its `TerminalSessionService` client. It carries the connection-chrome pattern (status dot, disconnect, terminate).

`ConnectionScreen` mounts `GhosttyTerminalSession` on the session's host-served terminal feed when the attached session's `agent` is `claude-cli`.

### Fullscreen renderer

Every `claude-cli` launch — a fresh start and a **Resume**, on the host and in the sandbox — runs
Claude's **fullscreen renderer**. It is the only renderer that switches to the alternate screen and
enables mouse tracking (DECSET `1049`, `1000`/`1002`, `1006`). The web terminal routes the wheel to
the TUI only once mouse tracking is on, and its live pane keeps no scrollback of its own, so an inline
Claude cannot be scrolled from the browser at all.

The renderer is forced by **`CLAUDE_CODE_NO_FLICKER=1`**, which the built-in `claude.yaml` declares
in its `run.env` (`#agent-tools` 19/19). Nothing outside the spec adds it: the host start, the
resume, the jailed start and the jail relaunch all take the agent's environment from `run.env`, so a
`<tddyhome>/tools/claude.yaml` override without it starts Claude inline.

daemon.yaml `claude_cli.fullscreen` is refused at start-up: the field moved to the spec's `run.env`
(`<tddyhome>/tools/claude.yaml`). Forcing the renderer also overrides Claude's own switch back to the
classic renderer after a fullscreen launch failed to start.

## Session table display

| Column | Claude CLI session value |
|--------|--------------------------|
| Workflow | `—` |
| Goal | `—` |
| Agent | `claude-cli` |
| Model | selected model label |
| Elapsed | `—` |
| Status | `active` / `inactive` |
| Activity | `Started` / `Running` / `ExecutingTool` / `WaitingForInput` / `Done` / `Ended` (see below) |

## Session activity status via per-worktree hooks

When the daemon starts a claude-cli session it writes `.claude/settings.local.json` into the git
worktree configuring eight Claude Code hooks. Each hook invokes
`tddy-tools session-hook` which maps the event to a granular `SessionActivityStatus` and calls
`ReportSessionStatus` on `activity.ActivityService`. The daemon validates the per-session
`hook_token`, writes `activity_status` to `.session.yaml`, and surfaces it via
`ListSessions.SessionEntry.activity_status`.

### Status mapping

| Claude hook event | `notification_type` | `activity_status` |
|---|---|---|
| `SessionStart` | — | `Started` |
| `UserPromptSubmit` | — | `Running` |
| `PostToolUse` | — | `ExecutingTool` |
| `Notification` | `permission_prompt` / `elicitation_dialog` / `idle_prompt` | `WaitingForInput` |
| `PermissionRequest` | — | `WaitingForInput`, with the tool and its input |
| `PreToolUse` (matcher `AskUserQuestion` only) | — | `WaitingForInput`, with every question and option |
| `Stop` | — | `Done` |
| `SessionEnd` | — | `Ended` |
| anything else | — | no-op (hook exits 0, no RPC) |

### What a session is asking

`PermissionRequest` and `PreToolUse(AskUserQuestion)` report the tool name and its input
(`ReportSessionStatusRequest.{hook_event_name, tool_name, tool_input_json}`). `PreToolUse` returns at
once with empty stdout and decides nothing; `PermissionRequest` then waits for an orchestrator's
answer when the session has one (below), and otherwise decides nothing either, so Claude's own
prompt appears. The daemon records the question
in the session's `pending-question.json` under a new `request_id` (returned in
`ReportSessionStatusResponse.request_id`) and sets `.session.yaml` `pending_elicitation`:

- **permission:** `kind: permission`, `tool_name`, `tool_input_json`, and `question` reading
  `Run <tool>: <summary>` — `Bash` is summarised by its `command`, `Edit`/`Write`/`Read` by their
  `file_path`, any other tool by its input as compact JSON;
- **question:** `kind: select` or `multi_select` (from the first question), `allow_other`, and
  `questions: [{header, question, options: [{label, description}], multi_select}]`, the flat fields
  mirroring the first.

A malformed `AskUserQuestion` input is refused as `InvalidArgument` before anything is written.
The `Notification` hook that follows the same prompt **keeps** the record (filling only an empty
`question` from its message). The next `PostToolUse`, `UserPromptSubmit`, `Stop` or `SessionEnd`
clears it and resets `pending_elicitation`. An orchestrator sees the record through
`ReadSessionActivity`. Neither hook of an `AskUserQuestion` is reported as a tool call
(`ReportAgentActivity`): it is a question, and a declined one has no `PostToolUse` to complete it,
so the activity log carries no row for it at all.

### Waiting for an orchestrator's answer

The `PermissionRequest` hook and the `PreToolUse` hook of `AskUserQuestion` (matched to that tool
alone) run `session-hook --await-answer --await-timeout-secs <wait + 5>`, with Claude's hook
`timeout` set to `<wait + 10>`, where `<wait>` is the agent spec's `coding_agent.answer_wait_secs`
(120 s when absent). The start records that wait beside the session (`answer-wait-ms`, next to
`.session.yaml`), and `AwaitQuestionAnswer` holds the hook for the recorded value, so the hold always
equals the hook timeout the agent was launched with; editing `answer_wait_secs` changes new sessions
only. A session with no record is held for its spec's value (`#agent-tools` 19/19). After its report it calls `ActivityService.AwaitQuestionAnswer` with the report's
`request_id` and, for a permission, Claude's `permission_suggestions` (as JSON; empty when none). A
`PreToolUse` of any other tool never waits. The daemon, with the
same authentication as `ReportSessionStatus`:

- answers `NO_ORCHESTRATOR` at once when the session's changeset names no `orchestrator_session_id`,
  or names one whose main terminal **the daemon it runs on** does not serve (a served terminal, not
  a live pid, so a dead orchestrator whose pid was reused never makes a child wait). An orchestrator
  on this host is judged from this daemon's own terminals. One on another host — the changeset's
  `orchestrator_daemon_instance_id` names a peer — is asked of that peer once, before the wait, with
  the peer-only `ActivityService.IsSessionTerminalServed`, bounded by 3 s (`PEER_PRESENCE_TIMEOUT`).
  A peer off the roster, unreachable, erroring or silent past the bound counts as absent and is
  logged at warn, so asking never delays Claude's prompt by more than the bound. The wait itself
  stays on the child's host; the orchestrator's `AnswerSessionPermission` / `AnswerSessionQuestion`
  is forwarded there. A peer that answers "served" and then goes away is not re-asked: the hook
  waits out `<wait>` and ends `TIMED_OUT`;
- refuses a `request_id` that is not the session's pending question;
- otherwise holds the call until the orchestrator answers → `ANSWERED`, or until `<wait>` passes →
  `TIMED_OUT`. A pending question of kind `permission` is answered with `AnswerSessionPermission`
  ([Project concept § Answering a permission prompt](project-concept.md#answering-a-permission-prompt)),
  and `ANSWERED` carries `option_id` or `cancelled`, `message` and `answered_by`. Any other kind is
  a question, answered with `AnswerSessionQuestion`
  ([Project concept § Answering a question](project-concept.md#answering-a-question)), and
  `ANSWERED` carries `answers` (one `{question_index, selected, other?}` per question answered) and
  `answered_by`.

The hook prints Claude's decision on `ANSWERED`, and nothing in every other case, so Claude's own
prompt follows:

| Answer | stdout |
|---|---|
| `allow_once` | `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}` |
| `allow_always` | the same, with `"updatedPermissions"`: Claude's `permission_suggestions` |
| `reject_*` | `"decision":{"behavior":"deny","message":"<message>"}` |
| `cancelled`, `TIMED_OUT`, `NO_ORCHESTRATOR`, a daemon error, the hook's own deadline | nothing |

For an `AskUserQuestion` the hook prints a `PreToolUse` decision only on `ANSWERED`, and only when
the answers fit the call's own `tool_input` (it has `questions`, and every answer's index is one of
them); otherwise nothing, and Claude shows its own dialog. The decision allows the call with
`updatedInput`: the call's input unchanged, plus an `answers` map from each question's text to its
answer — the chosen labels joined with `", "`, then the free `other` text, or `other` alone. Claude
then skips the dialog and runs the tool as if the person had answered (`User answered Claude's
questions: …`), with no denied call in the transcript. A question the answers leave out stays absent
from the map and Claude asks it. Verified interactively on Claude Code 2.1.312; `claude -p` does not offer
`AskUserQuestion` at all, so headless runs cannot check it.

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow",
  "updatedInput":{"questions":[…as asked…],"answers":{"Which language?":"Rust"}}}}
```

The bounds nest: the daemon answers `TIMED_OUT` before the hook's own deadline, and the hook ends
before Claude kills it. Waiters are held in memory, one per `request_id`: a daemon restart drops
them and each hook falls back to Claude's prompt. A person can still answer in Claude's terminal once
the wait ends.

### Hook wiring

`start_claude_cli_session` generates a per-session UUID `hook_token` and calls
`tddy_core::build_claude_hooks_settings(&HookCommandParams { tddy_tools_path, daemon_url, daemon_socket, session_id, os_user, hook_token, answer_wait_secs })`.
The resulting JSON is written to `<worktree>/.claude/settings.local.json`. The `hook_token` is also
stored in `.session.yaml` for validation on inbound `ReportSessionStatus` calls.

The `tddy-tools` binary is `binary.command` of `tools/tddy-tools.yaml` (default: `current_exe` sibling → `"tddy-tools"`), the hook's `daemon_url` is the daemon's `listen.advertise_url` (default: `http://127.0.0.1:{web_port}`) and `answer_wait_secs` is 120. A daemon.yaml `claude_cli:` block is refused at start-up.

**Capabilities.** The `claude` spec declares `hooks`, `ask_user_question`, `split_host`, `project_home_main`, `requires_model`, `branch_name_required` and `operator_notify`. Hook status reports need `hooks`; `AwaitQuestionAnswer` needs `ask_user_question`; operator notifications and the `notify` tool need `operator_notify`; a split or sandboxed-codebase placement needs `split_host`. Gates read the spec, not the session-type string.

### Which transport a hook reports over

Every hook command a host launch writes carries `--daemon <daemon_url> --daemon-socket '<path>'`, and
`session-hook` reports over the socket alone (a jailed launch points it at the in-jail tool-IPC socket
instead, which the runner relays). The path is the **agent-tool socket of the session's OS
user** — a Unix socket the daemon serves its full RPC roster on for co-located processes:

| Host | `--daemon-socket` |
|---|---|
| Tddy Desktop (embedded daemon) | the daemon's one socket, `agent_tool_socket_path(<resolved data dir>)`. An embedded daemon serves no HTTP RPC (its `listen.web_port` only answers a GitHub sign-in and returns 404 for `/rpc`), and a session started before the runtime serves the socket is refused rather than pointed at it |
| Server daemon (`tddy-daemon` binary) | `<data dir>/run-agent-tools/<os user>/agent.sock`: a directory `0700` and a socket `0600`, both owned by that user, so only that user (and root) can connect. Bound when the user's first session starts; the data dir is the one the daemon resolved (`TDDY_DATA_DIR` wins over `tddy_data_dir`) |
| Server daemon under `tddy-supervisor` | the socket the supervisor created for that user from `agent_tool_sockets:` in `supervisor.yaml` (the unprivileged daemon cannot `chown` one to another user). A user with no entry has the start **refused** (`FAILED_PRECONDITION`, naming `agent_tool_sockets`) — never started with hooks that report nowhere. See [tddy-supervisor § Per-user sockets](../supervisor/tddy-supervisor.md#per-user-sockets-handed-to-the-daemon) |

The socket path is resolved before anything of the start is created. Hooks are written at session
start, so a session keeps the socket it started with until it restarts. A server daemon that stops
removes the sockets it bound itself and leaves the supervisor's in place for the next daemon.

Split agents' hooks are written the same way. Cursor-cli hooks report over the agent-tool socket on
the host and over the tool-IPC bridge in a jail — see
[Cursor CLI session § Activity status hooks](cursor-cli-session.md#activity-status-hooks-cursorhooksjson).

### Fail-quiet contract

`tddy-tools session-hook` always exits 0. A 2-second ceiling on each report (the permission wait has its own, `--await-timeout-secs`) — over HTTP or the agent-tool socket — prevents a dead daemon from blocking Claude; a socket nobody listens on is refused at once. On no-op events (unrecognized event names, unrecognized `notification_type`) the tool exits immediately without making any network call.

### Auth model

The hook has no web session token. A per-session random `hook_token` (UUID) is generated at session start, persisted in `.session.yaml`, and baked into each hook command line. `ReportSessionStatus` resolves `sessions_base` directly from `os_user` (bypasses the GitHub OAuth path) and constant-time-compares the token. Sessions with no `hook_token` (e.g. Telegram-started) silently ignore inbound hook reports.

### Web UI

Web-side rendering of `activity_status` (badge in the session list) is a follow-up; this changeset surfaces the field in the `ListSessions` response but does not yet display it in the UI.

## Notifying the operator

A daemon-launched claude-cli agent can notify its operator in its own words with
`tddy-tools notify` (see
[Session notifications § Agent-written notifications](session-notifications.md#agent-written-notifications)).
The launch gives it two things, at start and again at resume:

- **The notify session** the command reads (`tddy_agent_launch::notify_env::notify_env_pairs`; the
  names live in `tddy_service::session_activity::notify_env`). It is not exported to the agent
  process: it is written into the session pointer's `notify` map
  ([§ The generic CLI spec launch](#the-generic-cli-spec-launch-serves-claudes-needs)), and the
  `tddy-tools notify` the agent runs adopts it from there (only these five names):

  | Variable | Value |
  |---|---|
  | `TDDY_SESSION_ID` | the session id |
  | `TDDY_OS_USER` | the OS user that owns the session |
  | `TDDY_HOOK_TOKEN` | the session's `hook_token` — the same credential its hooks present |
  | `TDDY_DAEMON_SOCKET` | the session OS user's agent-tool socket — the hooks' `--daemon-socket` |
  | `TDDY_DAEMON_URL` | the hooks' daemon URL, when no socket is given (never for a daemon-launched agent) |

  Exactly one of the last two is given: a daemon-launched agent always gets the socket, which is how
  an embedded daemon, which serves no HTTP RPC, is reached at all.
- **One guidance paragraph** in its system prompt (`agent_notify_guidance.md`): the command exists,
  use it when long work finishes or something blocks while the operator is likely away, never for
  routine progress. `claude` takes a single `--append-system-prompt-file`, so the daemon writes
  `<session dir>/agent-system-prompt.md` holding the guidance followed by a managed recipe's
  orchestration prompt, when the session has one.

A resume rebuilds both from the `hook_token` recorded in `.session.yaml`; a session that recorded
none is given nothing. Not wired:

- **A session with a standing role** (`session_role`, e.g. the project-home main). It has no shell to
  run the command, so neither the notify session nor the guidance is given. It notifies with the
  `NotifyOperator` exec tool instead, and its role prompt names that tool
  ([Session notifications § A session with no shell](session-notifications.md#a-session-with-no-shell-notifyoperator)).
- **A sandboxed session.** It has no hooks and no hook token.

A cursor-cli session gets the same notify session through its pointer at start and resume (its
socket and URL are its hooks').

## Proto delta

`connection.proto`:

```proto
message StartSessionRequest {
  // ... existing fields ...
  string session_type = 7;  // "tool" (default) or "claude-cli"
  string model        = 8;  // model id for claude-cli sessions
  bool   sandbox      = 15; // when true with session_type "claude-cli": darwin Seatbelt spawn (macOS only)
  bool   run_in_tmux  = 47; // claude-cli: run the agent in a tmux session on the host (see § tmux mode)
}

// SessionEntry (in ListSessionsResponse) gains:
//   string activity_status = 15;
```

`activity.proto` — the activity-status hooks, on `activity.ActivityService` (served by
`tddy-session-activity`; `#unbundle` node 7 moved them off `connection.ConnectionService`, so the
hook calls `activity.ActivityService/ReportSessionStatus` — over the session OS user's agent-tool socket; `{daemon_url}/rpc/…`
over HTTP for a hook command given no socket):

```proto
rpc ReportSessionStatus(ReportSessionStatusRequest) returns (ReportSessionStatusResponse);
rpc AwaitQuestionAnswer(AwaitQuestionAnswerRequest) returns (AwaitQuestionAnswerResponse);

message AwaitQuestionAnswerRequest {
  string session_id = 1;
  string hook_token = 2;
  string os_user = 3;
  string request_id = 4;                  // ReportSessionStatusResponse.request_id
  string permission_suggestions_json = 5; // Claude's permission_suggestions; empty when none
}

enum AwaitQuestionOutcome {
  AWAIT_QUESTION_OUTCOME_UNSPECIFIED = 0;
  AWAIT_QUESTION_OUTCOME_ANSWERED = 1;
  AWAIT_QUESTION_OUTCOME_NO_ORCHESTRATOR = 2;
  AWAIT_QUESTION_OUTCOME_TIMED_OUT = 3;
}

message AwaitQuestionAnswerResponse {
  AwaitQuestionOutcome outcome = 1;
  string option_id = 2;   // allow_once, allow_always, reject_once; empty when cancelled
  bool cancelled = 3;
  string message = 4;     // with a reject_* option
  string answered_by = 5; // the orchestrator's session id
}

message ReportSessionStatusRequest {
  string session_id  = 1;
  string hook_token  = 2;
  string os_user     = 3;
  string status      = 4;  // one of the SessionActivityStatus wire strings
}

message ReportSessionStatusResponse {
  bool ok = 1;
}
```

`StartSessionResponse`, `ConnectSessionResponse`, and `ResumeSessionResponse` are unchanged — LiveKit fields are returned as empty strings for Claude CLI sessions. The web client detects `agent == "claude-cli"` from `ListSessions` to decide which terminal component to mount.

## Seeding a first prompt

Both the RPC path and the Telegram path support an optional **initial prompt** passed directly to the `claude` binary as a positional argument:

```
claude --model <model> --session-id <id> "build feature X"
```

### RPC: `StartSessionRequest.initial_prompt`

`StartSessionRequest` has a `string initial_prompt = 13;` field. When non-empty, `start_claude_cli_session` passes it as a positional CLI argument to `claude`. An empty string is treated as absent (no extra arg). This lets programmatic callers seed the first user turn without requiring interactive input.

### Telegram: `/start-claude <prompt>`

The Telegram `/start-claude <prompt>` flow (see [telegram-session-control.md](telegram-session-control.md#cli-agent-starts-start-agent-start-claude-start-cursor)) seeds the session with the user's message. After project → branch → model selection (the models of `claude.yaml`), Telegram sends `initial_prompt` as the request's `initial_prompt` through the daemon's generic start, so the session gets the same spec launch, hooks file and hook token as a web start.

### Resume does not replay

`ResumeSession` always calls `ClaudeCliSessionManager::resume`, which **does not** pass the original `initial_prompt`. This prevents the first user message from being re-injected as a duplicate turn when `claude` is restarted inside an existing session.

### Implementation note

Argv construction is isolated in `build_claude_argv(binary, model, session_id, initial_prompt: Option<&str>) -> Vec<String>` in `claude_cli_session.rs`. The positional arg is appended after `--session-id` only when the trimmed prompt is non-empty.

## See also

- [Telegram session control](telegram-session-control.md) — `/start-claude` Telegram flow
- [Web terminal](../web/web-terminal.md) — GhosttyTerminal, connection chrome, session table
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md) — daemon RPC implementation
- [Worktrees](../web/worktrees.md) — worktree lifecycle, `create_worktree_with_retry`
