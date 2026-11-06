# Cursor Agent CLI Session

> **Related:** [Claude Code CLI session](claude-cli-session.md) — sibling session type; shares `CliSessionManager`, `StreamSessionTerminalIO`, and web terminal mounting.
> **Related:** [OpenCode CLI session](opencode-cli-session.md) — sibling session type; Cursor and OpenCode share the daemon-cut worktree provisioning (`cli_session_worktree.rs`).
>
> **Also runs as an agent tab:** the harness can be opened as an extra terminal inside another session, beside that session's main agent in its worktree — see [Session terminal tabs](../web/session-terminal-tabs.md).

## Summary

A session type that spawns an interactive Cursor Agent CLI process (`agent` binary) in a PTY within a git worktree, managed entirely by `tddy-daemon`. Users select **Cursor Agent CLI** as the session type from the web **Create session** pane, choose a Cursor-supported model, and get direct terminal access — without TDD workflow overhead. The daemon handles worktree creation, `.cursor/hooks.json` installation, PTY lifecycle, activity status, session persistence, and resume if the process exits.

## User Story

As a developer, I want to start a raw Cursor Agent CLI session from the tddy web UI (or Telegram) so I can interact with the Cursor agent interactively inside a managed worktree — with the same session persistence, activity status, and resume capabilities as Claude Code CLI sessions.

## Start session

1. The **Create session** pane's **Session type** selector includes **Cursor Agent CLI** (alongside **Managed session** and **Claude Code CLI**). When selected:
   - The **Workflow recipe** dropdown appears when **Managed codebase** is enabled.
   - A **Model** dropdown is populated from `ListAgentModels("cursor-cli")` (curated catalog in `tddy_core::cursor_cli_models()`).
   - The **Tool** dropdown is hidden: the agent is the `cursor` tool definition (see [Launch](#launch-from-the-cursor-tool-definition)).
   - **Sandbox** and **Managed codebase** toggles are available (parity with Claude CLI).
2. **Start New Session** issues `StartSession` with `session_type = "cursor-cli"`, the chosen `model`, and optional `sandbox`, `managed_codebase`, `recipe`, and `specialized_agents`.
3. The daemon creates a git worktree on branch `cursor-cli/{short_id}` (auto-default when `new_branch_name` is empty).
4. The daemon mints the session's Cursor chat (`agent create-chat`, the `cursor-create-chat` launch adapter) and records its id before the agent starts. The mint is bounded by 30 s: a `create-chat` that never answers is killed and the start is refused `DEADLINE_EXCEEDED`, naming the agent, the adapter and the deadline; nothing is spawned.
5. The daemon launches the agent from `cursor.yaml`: `agent --resume <chat> --model <model> [<prompt>]`, with `<worktree>/.cursor/hooks.json` rendered for it.
6. `StartSessionResponse` returns `session_id`; LiveKit fields are empty.

## Terminal access

- The web UI mounts `GhosttyTerminalSession` connected to `StreamSessionTerminalIO` (same component path as claude-cli).
- No LiveKit connection is made for Cursor CLI sessions.

## Session persistence

The session directory `~/.tddy/sessions/<session-id>/` contains `.session.yaml` with:

- `session_type: cursor-cli`
- `model: <model-id>`
- `repo_path: <worktree-absolute-path>`
- `pid: <agent-pid>`
- `hook_token: <per-session-uuid>` (written before the agent starts: its first hook fires during the launch)
- `cursor_chat_id: <chat-id>` (the chat every resume reattaches to)
- `status: active`

`ListSessions` shows `workflow_goal`, `workflow_state`, and `elapsed_display` as `—`, `model` as the selected model, and `agent` as `cursor-cli`.

## Resume, connect, signal, delete

- **Resume** — when the agent process exits, status becomes `inactive`; **Resume** relaunches the `cursor` definition on the host in the existing worktree, into the recorded chat (`--resume <cursor_chat_id>`), without replaying the initial prompt. It keeps the hooks file its start wrote. A session recorded with no chat id adopts a fresh one on its first resume and pins it.
- **Connect** — when the process is still running, `ConnectSession` reattaches to the existing PTY.
- **Signal** — `SignalSession` delivers to the agent PTY process.
- **Delete** — SIGTERM, wait, remove session directory and worktree.

## tmux mode

A cursor-cli session started with **Run the agent in tmux** (`run_in_tmux = true`) runs the Cursor
Agent CLI in a detached tmux session on the host, with everything
[Claude CLI § tmux mode](claude-cli-session.md#tmux-mode-startsessionrequestrun_in_tmux--true) describes — the `tddy-<session id>` name
recorded as `tmux_session`, the pane's environment file, the pane pid as `pid`, delete, the agent
outliving its client and its daemon, client-vs-agent exit and the detach watcher — and the same
refusals with `sandbox`, `sandboxed_codebase` and a split placement.

- **Identity.** In tmux mode the agent runs as the **session's OS user**. Outside tmux it still runs as
  the daemon user.
- **Launch.** The argv is the PTY start's: `--model <model> --resume <chat id>` and the first prompt,
  with the chat minted beforehand as for every start.
- **Resume** re-attaches while the tmux session is alive (the agent is not relaunched); otherwise it
  starts a fresh tmux session running `--resume <chat id>`.
- **Re-attach after a restart.** The first stream, input or history read of the session's `main`
  terminal on a daemon that has none re-attaches it, dispatched by the session type `.session.yaml`
  records, so the main keeps its kind (`cursor-cli`). The agent is never relaunched and the recorded `pid`
  is unchanged.
- cursor-cli registers no exit hook, so its pane is not kept after the agent exits.

## Activity status hooks (`.cursor/hooks.json`)

At session start the `cursor-hooks-json` hooks adapter renders `<worktree>/.cursor/hooks.json`. Each hook invokes `tddy-tools session-hook --daemon-socket <socket>`, which reads Cursor's stdin JSON `hook_event_name`, maps it to `SessionActivityStatus`, and calls `ReportSessionStatus` (and `ReportAgentActivity`) on `activity.ActivityService`. The daemon validates `hook_token`, writes `activity_status` to `.session.yaml`, and surfaces it via `ListSessions`.

Where the reports go:

| Launch | `--daemon-socket` | Path to the daemon |
|---|---|---|
| Host | the daemon's agent-tool socket, `agent_tool_socket_path(<resolved data dir>)` | the socket itself |
| Jail | the in-jail tool-IPC socket | `tddy-sandbox-runner` relays the two reports (`IN_JAIL_RELAYABLE_HOOK_REPORTS`) to the facilitating daemon as host RPCs. The bridge serves them for the jail's own session only, and refuses any other session id |

A hooks file is never rendered without a socket: the adapter refuses (`SlotError::NoDaemonSocket`) rather than falling back to `daemon_url`.

**Known gap — server daemons.** Only a daemon embedded in an application (Tddy Desktop) binds the agent-tool socket. On a server (`tddy-daemon` binary) daemon, a **host** Cursor session's hook reports go to a socket nothing serves and are dropped fail-quiet, so its activity status does not update. Jailed sessions are unaffected. Binding it on server daemons needs one socket per OS user first (sessions under `tddy-supervisor` run as other users): [`docs/dev/todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md`](../../dev/todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md).

| Cursor hook event | `activity_status` |
|---|---|
| `sessionStart` | `Started` |
| `beforeSubmitPrompt` | `Running` |
| `postToolUse` | `ExecutingTool` |
| `stop` | `Done` |
| `sessionEnd` | `Ended` |
| `preToolUse` / `beforeShellExecution` / `afterShellExecution` / others | no-op (exit 0, no RPC) |

**`WaitingForInput`** is not produced — Cursor hooks have no permission-prompt equivalent in v1.

Hooks use a fail-quiet contract (a 2-second ceiling per report, exit 0 on no-op events), matching the Claude CLI hook path.

## Launch from the `cursor` tool definition

Every `cursor-cli` launch — host start, host resume, jailed start — renders from the built-in `packages/tddy-coding-agent-specs/specs/cursor.yaml` (overridable whole by `<tddyhome>/tools/cursor.yaml`) through the shared launchers: `launch_host_pty` on the host, `launch_jailed` in a jail. A host without coding agents refuses the start or resume with `FailedPrecondition: this host has no coding agents installed`; there is no hand-built fallback.

| Fact | Declared as |
|---|---|
| Binary | `agent`, overridable by `TDDY_CURSOR_AGENT`; a `<tddyhome>/tools/cursor.yaml` override sets `binary.command` |
| Argv | `agent [--resume {chat_id}] [--model {model}] <slots> [{prompt}]`. When the resolved binary is the published launcher script, the bundled `node` and its `index.js` run instead (`node_launcher` / `node_entry`) — the wrapper cannot resolve its own install dir in a jail |
| MCP | `kind: placed`, `{home}/.cursor/mcp.json`, format `cursor-mcp-json`. Rendered in a jail only, into the persistent home; a **host** launch registers no MCP server, so the user's own `$HOME/.cursor/mcp.json` is never touched |
| Instructions | `rules_file: .cursor/rules/tddy-managed-workflow.mdc` — the managed workflow's prompt, written whole on every launch |
| Hooks | `adapter: cursor-hooks-json` (above) |
| Tool allow / deny / permission prompt | `unsupported`: a launch that fills one is refused, never dropped. Managed-codebase enforcement reads this — see [`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`](../../dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) |
| Models | default `composer-2.5`, plus a curated list |
| Conversation | `conversation: { kind: minted, adapter: cursor-create-chat }` — the chat minted before the first start (`mint_deadline_secs`, default 30), recorded as `cursor_chat_id` and bound as `chat_id` on every launch |
| Capabilities | `hooks`, `managed_workflow`, `requires_model`, `keep_worktree_on_delete`, `operator_notify` |

The generic CLI launch serves the same needs from the spec alone: a copy of `cursor.yaml` dropped in under another id mints and records its chat, resumes it with `--resume <chat>`, writes `.cursor/hooks.json` and binds the node launcher, exactly as `cursor-cli` does ([Claude CLI session § The generic CLI spec launch](claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs)). A generic resume of a minted conversation whose session recorded no chat is refused `FAILED_PRECONDITION`. The built-in `cursor` id still dispatches to the `cursor-cli` launch.

## Telegram `/start-cursor`

`/start-cursor <prompt>` follows **project → branch → model** (`tcur:` callbacks), then spawns with the prompt as a positional arg. See [telegram-session-control.md](telegram-session-control.md#start-cursor-flow).

## Architecture

### Session type discriminant

`StartSessionRequest.session_type` accepts `"cursor-cli"`. The daemon branches in `connection_service.rs` via `cursor_cli_spawn.rs`. Existing `tool` and `claude-cli` sessions are unaffected.

### Session manager

`CliSessionManager` in `packages/tddy-daemon/src/cli_session_manager.rs` (re-exported as `claude_cli_session`) handles both `claude-cli` and `cursor-cli` PTY sessions. `launch_host_pty` registers a Cursor PTY with `PtyHandle.kind = "cursor-cli"`.

### Hook wiring

- `tddy_coding_agent_specs::slots::cursor::render_cursor_hooks_json` (`cursor-hooks-json`) — emits `.cursor/hooks.json` v1 (five lifecycle events), each naming the daemon socket.
- `tddy-tools session-hook` — primary mapping from stdin `hook_event_name`; `--event` fallback for tests.
- `tddy_core::session_activity` — unified PascalCase + camelCase hook name → status mapping shared with Claude hooks.

### Config

`daemon.yaml` has no `cursor_cli:` block; one is refused at start-up
([Removed `daemon.yaml` sections](systemd-install.md#removed-daemonyaml-sections)). The Cursor binary
is `binary.command` of `cursor.yaml` (`<tddyhome>/tools/cursor.yaml` overrides it) or `TDDY_CURSOR_AGENT`;
the Cursor home is the spec's `home.key`; the `--daemon` URL hooks carry is the daemon's
`listen.advertise_url`, unread when `--daemon-socket` is given, which every rendered hook is.

The `cursor` spec declares the capabilities `hooks`, `requires_model`, `keep_worktree_on_delete` and
`operator_notify`: hook status reports and operator notifications are accepted from a `cursor-cli`
session, a model is required to start one, and `DeleteSession` leaves its worktree in place.

### Web UI

**CreateSessionPane** is the primary entry point for the third session type. Terminal mount helpers in `ConnectionScreen` and `SessionsDrawerScreen` treat `cursor-cli` like `claude-cli` for gRPC terminal attachment.

Cursor's agent cannot be pointed at another provider, so a cursor-cli session always runs on Cursor. The form's **Provider** field shows, in place of a select, the reason the daemon gives (`ListProviderRoutes` answers `harness_routable = false` for `cursor-cli`) — see [provider accounts § Running a harness on an account](provider-accounts.md#running-a-harness-on-an-account).

### Proto

No new RPCs. `sandbox = true` with `session_type = "cursor-cli"` starts a Seatbelt (macOS) or cgroups+namespaces (Linux) sandboxed session, mirroring `claude-cli`.

### Sandbox mode

When `StartSession.sandbox = true`:

- The jail is lowered from `cursor.yaml`'s generic sections by `launch_jailed`, into an exec plan `tddy-sandbox-runner` runs. No Cursor-specific flag (`--agent-kind cursor`, `--cursor-home-dir`, …) reaches the runner from the daemon, and the Claude env overlay is not applied.
  - `home: { kind: persistent, key: cursor }` — `<tddyhome>/homes/cursor`, mounted read-write, so Cursor's refreshed login and config survive the session.
  - `files:` — `~/.cursor/auth.json` and `~/.cursor/cli-config.json` seeded once from the host, the `agent` install mirrored, and the overlays `CURSOR_TMPDIR`, `CURSOR_SKIP_KEYCHAIN=1` and `CI=true`.
  - `network.egress.hosts` — Cursor's API, auth and update hosts; an undeclared host is refused. A definition with no `network:` section cannot start a jail ([`2026-11-03-jail-with-no-network-section-cannot-bind-its-shim.md`](../../dev/todo/2026-11-03-jail-with-no-network-section-cannot-bind-its-shim.md)). The list is not yet verified against a live Cursor session (`TODO(cursor-cli)` in `cursor.yaml`).
- macOS: Seatbelt. Linux: rootless cgroups+user namespaces (same backend as Claude CLI).
- MCP config: `$HOME/.cursor/mcp.json` in the persistent home registers `tddy-tools --mcp`. Headless approval flags (`--approve-mcps`, `--force`, `--trust`) are **not** injected by tddy — pass them explicitly via agent args when using print mode.
- Egress is confined to the SessionChannel tunnel; tool execution uses the host worktree via IPC.

### Managed codebase and specialized subagents

When `managed_codebase = true`, the daemon seeds `changeset.yaml`, writes orchestration guidance to `.cursor/rules/tddy-managed-workflow.mdc` in the worktree (the definition's `rules_file`), sets `TDDY_SOCKET`, and exposes `subagent_new_session` / `prompt` / `cancel` via MCP. `specialized_agents` resolves to `TDDY_SUBAGENTS_JSON` / `TDDY_SUBAGENT` in the `tddy-tools` MCP server's env; the jailed agent's own process sees no `TDDY_*` variable.

### Workflow `CursorBackend::invoke`

`CursorBackend::invoke` registers `tddy-tools --mcp` and `permission-prompt-tool` for headless tool approvals (same model as `ClaudeCodeBackend`), and exports `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, and `TDDY_REMOTE_*` (`RemoteToolEnv`) into the invoke subprocess.

## Known follow-ups

- **Jail authentication:** a sandboxed `agent` cannot reach macOS Keychain — the host (`tddy-daemon` / `tddy-sandbox-app`) is an unsigned Rust binary with no `keychain-access-group` entitlement, so `securityd` rejects `SecItem*` calls with `errSecMissingEntitlement` (-34018) and the CLI hangs/times out ("Failed to store authentication tokens: Security command failed: Security process exited with code: 36"). `cursor.yaml`'s `files:` overlays set `CURSOR_SKIP_KEYCHAIN=1` + `CI=true` (Cursor's own CI convention) so the agent never probes Keychain. The caller must still supply `CURSOR_API_KEY` (User API key from the Cursor dashboard — admin keys are rejected) or pre-seed `~/.cursor/auth.json` (seeded once into the persistent home). True Keychain *access* from the jail would require codesigning the host with `keychain-access-group` and a TCC grant — recorded as a follow-up, not pursued; the current stance is fail-closed with `CURSOR_API_KEY`/seeded `auth.json`.
- **Jailed resume:** a sandboxed cursor-cli resume relaunches on the host PTY; a jailed resume is not implemented.
- **Server daemons:** host Cursor hook reports are dropped (see [Activity status hooks](#activity-status-hooks-cursorhooksjson)).

## Out of scope (v1)

- **`WaitingForInput`** activity status (Cursor hooks have no permission-prompt equivalent — documented gap)
- TDD/bugfix workflow integration (no recipe)
- Model switching after session start
- Session chaining (`previous_session_id`)
- LiveKit presence for cursor-cli processes
- **`ConnectionScreen`** legacy inline start form third session type (CreateSessionPane is primary)

## References

- [Claude Code CLI session](claude-cli-session.md)
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md)
- [Telegram session control](telegram-session-control.md)
- [Cursor Hooks docs](https://cursor.com/docs/hooks.md)
- [Cursor CLI headless docs](https://cursor.com/docs/cli/headless)
