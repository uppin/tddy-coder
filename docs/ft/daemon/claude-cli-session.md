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
| Network from agent | Host network | No direct egress (Seatbelt `deny network*` / Linux network namespace); LLM HTTPS tunneled over `SessionChannel` to a host relay (TLS end-to-end) |
| Control channel | — | loopback TCP (macOS) / AF_UNIX (Linux, survives the network namespace) |
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

The host's `CLAUDE_CODE_OAUTH_TOKEN` is not injected beside a route, and the jail's OAuth inbound port
is not opened. A resume relaunches the jail on the account the start recorded, re-resolved as below.
A sandboxed start that chooses nothing is unchanged.

A **child** session a pr-stack orchestrator spawns, and a **conversation** a grill-me orchestrator
spawns, run on the vendor: the orchestrator's account is not inherited.

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

The daemon forces the renderer with **`CLAUDE_CODE_NO_FLICKER=1`** in the agent's environment. On the
host the agent also inherits the daemon's environment; a sandboxed agent's environment is constructed,
so the variable has to be added there too. Both paths add it from one helper,
`DaemonConfig::claude_cli_renderer_env`.

```yaml
claude_cli:
  fullscreen: false   # default true; also true when `claude_cli:` is absent
```

`false` leaves the renderer to Claude (its own setting, or `/tui`). Forcing it also overrides Claude's
own switch back to the classic renderer after a fullscreen launch failed to start, so a machine where
fullscreen crashes needs `fullscreen: false`.

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
`timeout` set to `<wait + 10>`, where `<wait>` is `claude_cli.answer_wait_secs`
(default 120). After its report it calls `ActivityService.AwaitQuestionAnswer` with the report's
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

Config (`claude_cli:` block) may override `tddy_tools_path` (default: `current_exe` sibling → `"tddy-tools"`), `daemon_url` (default: `http://127.0.0.1:{web_port}`) and `answer_wait_secs` (default 120).

### Which transport a hook reports over

| Host | Hook command carries | Reports go over |
|---|---|---|
| Binary daemon (`tddy-daemon`) | `--daemon <daemon_url>` | HTTP: `{daemon_url}/rpc/activity.ActivityService/<method>` |
| Tddy Desktop (embedded daemon) | `--daemon <daemon_url> --daemon-socket '<path>'` | The daemon's **agent-tool socket** — the Unix socket an embedded daemon binds for co-located processes, serving its full RPC roster |

An embedded daemon serves no HTTP RPC: its `listen.web_port` is only where a GitHub sign-in returns,
and answers `/rpc` with 404. The socket is passed exactly when one is **listening** at
`agent_tool_socket_path(<resolved data dir>)` at session start (`embedded_host_agent_tool_socket`) —
a live connect, not the file existing, because the embedded daemon does not unlink the socket on
exit and a binary daemon later started on the same data dir must keep using HTTP. The data dir is the
one the daemon resolved (`TDDY_DATA_DIR` wins over `tddy_data_dir` in the config), the same one it
bound the socket under. When `--daemon-socket` is given, `session-hook` uses only the socket. Hooks
are written at session start, so a session keeps the transport it started with until it restarts.

Split agents' hooks are written the same way. Cursor-cli hooks still report over HTTP only — see
[`docs/dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md`](../../dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md).

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

- **The environment** the command reads (`tddy_agent_launch::notify_env::notify_env_pairs`; the names
  live in `tddy_service::session_activity::notify_env`):

  | Variable | Value |
  |---|---|
  | `TDDY_SESSION_ID` | the session id |
  | `TDDY_OS_USER` | the OS user that owns the session |
  | `TDDY_HOOK_TOKEN` | the session's `hook_token` — the same credential its hooks present |
  | `TDDY_DAEMON_SOCKET` | the embedded daemon's agent-tool socket, when one is listening (the same rule as the hooks' `--daemon-socket`) |
  | `TDDY_DAEMON_URL` | otherwise, the hooks' daemon URL |

  Exactly one of the last two is exported: an embedded daemon serves no HTTP RPC, so a URL there would
  reach nothing.
- **One guidance paragraph** in its system prompt (`agent_notify_guidance.md`): the command exists,
  use it when long work finishes or something blocks while the operator is likely away, never for
  routine progress. `claude` takes a single `--append-system-prompt-file`, so the daemon writes
  `<session dir>/agent-system-prompt.md` holding the guidance followed by a managed recipe's
  orchestration prompt, when the session has one.

A resume rebuilds both from the `hook_token` recorded in `.session.yaml`; a session that recorded
none is given nothing. Not wired:

- **A session with a standing role** (`session_role`, e.g. the project-home main). It has no shell to
  run the command, so neither the environment nor the guidance is given. It notifies with the
  `NotifyOperator` exec tool instead, and its role prompt names that tool
  ([Session notifications § A session with no shell](session-notifications.md#a-session-with-no-shell-notifyoperator)).
- **A sandboxed session.** It has no hooks and no hook token.

A cursor-cli session is given the same environment at start and resume (its socket and URL are its
hooks'), but no guidance paragraph: its CLI has no system-prompt flag this relies on.

## Proto delta

`connection.proto`:

```proto
message StartSessionRequest {
  // ... existing fields ...
  string session_type = 7;  // "tool" (default) or "claude-cli"
  string model        = 8;  // model id for claude-cli sessions
  bool   sandbox      = 15; // when true with session_type "claude-cli": darwin Seatbelt spawn (macOS only)
}

// SessionEntry (in ListSessionsResponse) gains:
//   string activity_status = 15;
```

`activity.proto` — the activity-status hooks, on `activity.ActivityService` (served by
`tddy-session-activity`; `#unbundle` node 7 moved them off `connection.ConnectionService`, so the
hook calls `activity.ActivityService/ReportSessionStatus` — over HTTP at `{daemon_url}/rpc/…`, or over
the agent-tool socket on Tddy Desktop):

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

The Telegram `/start-claude <prompt>` flow (see [telegram-session-control.md](telegram-session-control.md)) seeds the session with the user's message. After project → branch → model selection, `spawn_telegram_claude_cli` reads `initial_prompt` from `changeset.yaml` and passes it to `ClaudeCliSessionManager::start(…, initial_prompt)`.

### Resume does not replay

`ResumeSession` always calls `ClaudeCliSessionManager::resume`, which **does not** pass the original `initial_prompt`. This prevents the first user message from being re-injected as a duplicate turn when `claude` is restarted inside an existing session.

### Implementation note

Argv construction is isolated in `build_claude_argv(binary, model, session_id, initial_prompt: Option<&str>) -> Vec<String>` in `claude_cli_session.rs`. The positional arg is appended after `--session-id` only when the trimmed prompt is non-empty.

## See also

- [Telegram session control](telegram-session-control.md) — `/start-claude` Telegram flow
- [Web terminal](../web/web-terminal.md) — GhosttyTerminal, connection chrome, session table
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md) — daemon RPC implementation
- [Worktrees](../web/worktrees.md) — worktree lifecycle, `create_worktree_with_retry`
