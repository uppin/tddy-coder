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

`ListSessions` shows `workflow_goal`, `workflow_state`, and `elapsed_display` as `—`, `model` as the recorded model (the selected one, until the agent reports another — see [Resume](#resume-connect-signal-delete)), and `agent` as `cursor-cli`.

## Resume, connect, signal, delete

- **Resume** — when the agent process exits, status becomes `inactive`; **Resume** relaunches the `cursor` definition on the host in the existing worktree, into the recorded chat (`--resume <cursor_chat_id>`), without replaying the initial prompt. It keeps the hooks file its start wrote. A session recorded with no chat id adopts a fresh one on its first resume and pins it.
  The resume runs `--model` with the model the session last reported, not the one it started on: `cursor.yaml`'s `coding_agent.model_report` reads the `model` field of the main conversation's own hook events (`sessionStart`, `beforeSubmitPrompt`, `stop` — see [Activity status hooks](#activity-status-hooks-cursorhooksjson)), and a value differing from `.session.yaml` `model` replaces it. A model picked with `/model` inside the TUI therefore survives a resume. Cursor's `/model` also rewrites its global default (`~/.cursor/cli-config.json`), so leaving `--model` off a resume would land on whatever another session picked last; only the session's own report says what it was on. The model is recorded as Cursor reports it: `auto` reports itself as `auto-smart` (an operator decision to keep it), and `agent --model` takes the reported ids.
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
  terminal on a daemon that has none re-attaches it as the agent the session type `.session.yaml`
  records names among the host's installed coding agents, so the main keeps its kind (`cursor-cli`).
  A record without a session type is skipped with a warning, and a host without coding agents
  re-attaches nothing. The agent is never relaunched and the recorded `pid`
  is unchanged.
- cursor-cli registers no exit hook, so its pane is not kept after the agent exits.

## Activity status hooks (`.cursor/hooks.json`)

At session start the `cursor-hooks-json` hooks adapter renders `<worktree>/.cursor/hooks.json`. Each hook invokes `tddy-tools session-hook --daemon-socket <socket>`, which reads Cursor's stdin JSON `hook_event_name`, maps it to `SessionActivityStatus`, and calls `ReportSessionStatus` (and `ReportAgentActivity`) on `activity.ActivityService`. The daemon validates `hook_token`, writes `activity_status` to `.session.yaml`, and surfaces it via `ListSessions`.

Where the reports go:

| Launch | `--daemon-socket` | Path to the daemon |
|---|---|---|
| Host | the agent-tool socket of the session's OS user: Tddy Desktop's one socket, or on a server daemon `<data dir>/run/<user>/daemon.sock` (owner-only; under `tddy-supervisor` the one it handed over, `/run/tddy-users/<user>/daemon.sock`) | the socket itself |
| Jail | the in-jail tool-IPC socket | `tddy-sandbox-runner` relays the two reports (`IN_JAIL_RELAYABLE_HOOK_REPORTS`) to the facilitating daemon as host RPCs. The bridge serves them for the jail's own session only, and refuses any other session id |

A hooks file is never rendered without a socket: the adapter refuses (`SlotError::NoDaemonSocket`) rather than falling back to `daemon_url`. On a target session the carrier stands in for the socket: with `HookCommandParams.remote_log` set, each event's command appends to `.tddy/hooks.log` in the worktree (see [Claude CLI session § Hook wiring](claude-cli-session.md#hook-wiring)), and no socket is required.

**Server daemons** serve one daemon socket per OS user, so a host session's reports reach the daemon whichever user it runs as, and no other user can connect to it. Under `tddy-supervisor` a session user must be mapped in `users.yaml`; an unmapped one has the start refused, naming `users.yaml`, rather than started with hooks that report nowhere — see [tddy-supervisor § One socket per user](../supervisor/tddy-supervisor.md#one-socket-per-user-handed-to-the-daemon).

| Cursor hook event | `activity_status` | agent activity |
|---|---|---|
| `sessionStart` | `Started` (fresh-chat launches only — see below) | — |
| `beforeSubmitPrompt` | `Running` | — |
| `preToolUse` | no-op (exit 0, no status RPC) | opens the call's `running` row |
| `postToolUse` | `ExecutingTool` | closes the row (`completed`, or `error` when the output flags one) |
| `postToolUseFailure` | no-op (exit 0, no status RPC) | closes the row as `error` (fires *instead of* `postToolUse`) |
| `stop` | `Done` | — |
| `sessionEnd` | `Ended` | — |
| `beforeShellExecution` / `afterShellExecution` / `beforeMCPExecution` / `afterMCPExecution` / `afterFileEdit` / `afterAgentResponse` / others | not installed | not installed |

The pre/post pair names the agent's id for the call (Cursor's `tool_use_id`), which pairs
the two rows; a call whose ids interleave still pairs. The uninstalled hooks fire alongside
the pair for the same tool call — or carry no tool payload at all — so wiring one would
duplicate a row the pair already reports.

**No `sessionStart` on a resumed launch.** Every tddy Cursor launch resumes its chat
(`agent --resume <chat>`, minted before the first start), and a resumed launch never fires
`sessionStart` — the first hook to arrive is `beforeSubmitPrompt` (`Running`). The daemon
therefore records `Started` in `.session.yaml` at spawn instead of waiting for the hook.

**`WaitingForInput`** is not produced — Cursor hooks have no permission-prompt equivalent in v1.

**The model the session is on.** Every Cursor hook event carries the session's current `model` (`glm-5p3-flash-high` after picking GLM 5.3 Flash in `/model`), and `ReportSessionStatus` carries it in `hook_fields`. `cursor.yaml` declares `model_report: [{ kind: hook_field, field: model, events: [sessionStart, beforeSubmitPrompt, stop] }]`, so the daemon records it in `.session.yaml` `model` — read on the main conversation's own events only, so a subagent's tool call on another model never re-points the session. Since a tddy launch fires no `sessionStart` (above), the report arrives with `beforeSubmitPrompt` and `stop`. See [Coding agent specs § Model report](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md#model-report-coding_agentmodel_report).

Hooks use a fail-quiet contract (a 2-second ceiling per report, exit 0 on no-op events), matching the Claude CLI hook path.

## Activity from the transcript

Cursor's hooks report its tool calls (ids, status, results) and nothing it says. Its own transcript —
`~/.cursor/projects/<worktree slug>/agent-transcripts/<chat>/<chat>.jsonl` — holds every prompt and
every reply, the text between tool calls included, but no tool results and no tool ids. So
`cursor.yaml` declares `coding_agent.activity: { messages: transcript, tool_calls: hooks }`, and the
session's feed (`ReadSessionTranscript`, `ReadSessionActivity`, the web Agent Activity panel) carries
the whole conversation in Cursor's order — `user_message`, `agent_message`, `tool_call` … — without
anyone reading the terminal. How a line maps is declared in the same section, read by one generic
mapper; see [Coding agent specs § Activity](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md#activity-coding_agentactivity).

| Cursor transcript line | Becomes |
|---|---|
| `{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>…</timestamp>\n<user_query>…</user_query>"}]}}` | a `user_message`, the inside of `<user_query>` |
| `{"role":"assistant",…{"type":"text",…}}` | an `agent_message` per text part |
| `{"role":"assistant",…{"type":"tool_use","name":…,"input":…}}` | nothing of its own: paired with the hook row of the same call (it places the text around the calls) |
| `{"type":"turn_ended",…}` | the turn's end for the pairing |

- **Where.** The path is rendered from the session's `usage-context.json` (chat id, worktree): the
  project directory is the worktree path with every run of non-alphanumerics made one `-`. On a target
  (SSH or Coder) the transcript is the target's: its home is the target shell's `$HOME`, its worktree
  the target's path, and it is followed through the session's remote shell; on this host it is followed
  through `/bin/sh`. Both use the hook log's follower — see
  [Claude CLI session § Following a target's hook log](claude-cli-session.md#following-a-targets-hook-log).
- **When.** The follower starts with the agent (start, resume, re-attach) and waits for a file that
  does not exist yet (`tail -F`: the transcript appears with the first prompt). Its offset is persisted
  in `<session_dir>/followers/transcript.offset` and kept across the session's end — the chat's file
  outlives the session, and a resume appends to it — so no line is read twice or skipped. A target's
  `tail` lives while the session's hook log exists there; a host one while
  `<session_dir>/followers/transcript.alive` does.
- **Order.** A follower may read a line long after it was written (it was restarted, or the target is
  slow). What the transcript says next happened before every hook call it has not reached yet, so each
  message is stamped no later than the earliest such call's start. The replay places a tool call at its
  completion, so text between two calls that ran **in parallel** is placed before the later one's
  start.
- **Call ids.** Cursor's `tool_use_id`s embed a `\n`; every control or whitespace character of a
  reported id is recorded as `_`.

## Launch from the `cursor` tool definition

Every `cursor-cli` launch — host start, host resume, re-attach, jailed start and resume — is the one generic CLI launch every agent takes, rendering the built-in `packages/tddy-coding-agent-specs/specs/cursor.yaml` (overridable whole by `<tddyhome>/tools/cursor.yaml`) through the shared launchers: `launch_host_pty` on the host, `launch_jailed` in a jail. A host without coding agents refuses the start, resume or child with `FailedPrecondition: this host has no coding agents installed`; there is no hand-built fallback.

| Fact | Declared as |
|---|---|
| Binary | `agent`, overridable by `TDDY_CURSOR_AGENT`; a `<tddyhome>/tools/cursor.yaml` override sets `binary.command` |
| Argv | `agent [--resume {chat_id}] [--model {model}] <slots> [{prompt}]`. When the resolved binary is the published launcher script, the bundled `node` and its `index.js` run instead (`node_launcher` / `node_entry`) — the wrapper cannot resolve its own install dir in a jail |
| MCP | `kind: placed`, `{home}/.cursor/mcp.json`, format `cursor-mcp-json`. Rendered in a jail only, into the persistent home; a **host** launch registers no MCP server, so the user's own `$HOME/.cursor/mcp.json` is never touched |
| Instructions | `rules_file: .cursor/rules/tddy-managed-workflow.mdc` — the managed workflow's prompt, written whole on every launch |
| Hooks | `adapter: cursor-hooks-json` (above) |
| Activity | `activity: { messages: transcript, tool_calls: hooks }`, with the transcript's path and line rules ([above](#activity-from-the-transcript)) |
| Tool allow / deny / permission prompt | `unsupported`: a launch that fills one is refused, never dropped. Managed-codebase enforcement reads this — see [`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`](../../dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) |
| Models | default `composer-2.5`, plus a curated list |
| Conversation | `conversation: { kind: minted, adapter: cursor-create-chat }` — the chat minted before the first start (`mint_deadline_secs`, default 30), recorded as `cursor_chat_id` and bound as `chat_id` on every launch |
| Capabilities | `hooks`, `managed_workflow`, `requires_model`, `keep_worktree_on_delete`, `operator_notify` |

The generic CLI launch serves the same needs from the spec alone: a copy of `cursor.yaml` dropped in under another id mints and records its chat, resumes it with `--resume <chat>`, writes `.cursor/hooks.json` and binds the node launcher, exactly as `cursor-cli` does ([Claude CLI session § The generic CLI spec launch](claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs)). A resume of a session that recorded no chat adopts a freshly minted one and records it, so every later resume continues it. Nothing in the launch compares the agent's id.

Like every CLI agent, a cursor-cli agent starts from an allow-listed environment (its spec's `pass_env` names `CURSOR_API_KEY`; [Harness profiles § The agent's environment](harness-profiles.md#the-agents-environment)) and gets no `TDDY_*` variable in its process; its hooks carry the session over `--daemon-socket`, and a `tddy-tools` it runs from its shell (`notify`, or `submit` / `ask` / `transition` in a managed session) finds the session through the `.tddy/session.json` pointer the launch writes into the worktree ([Claude CLI session § The generic CLI spec launch](claude-cli-session.md#the-generic-cli-spec-launch-serves-claudes-needs)).

A conversation or pr-stack child of a cursor-cli orchestrator is itself a cursor-cli session: a child runs its parent's agent.

## Telegram `/start-cursor`

`/start-cursor <prompt>` pre-picks the `cursor` agent and follows **project → branch → model** — the models of `cursor.yaml` (`tcm:` callbacks) — then starts through the daemon's generic start, like a web start: the spec's argv, `.cursor/hooks.json`, and a chat minted by `cursor-create-chat` under its deadline. See [telegram-session-control.md](telegram-session-control.md#cli-agent-starts-start-agent-start-claude-start-cursor).

## Architecture

### Session type discriminant

`StartSessionRequest.session_type` accepts `"cursor-cli"`, the session type `cursor.yaml` declares (`extensions.session`, `launch: agent_cli`; see [Session types are declared](session-agent-roster.md#session-types-are-declared)). Like every session type declared with the `agent_cli` launch, it resolves through the host's coding agents (`harness_from_spec::requested_session_launch` → `SessionLaunch::AgentCli`) to the generic CLI launch (`opencode_cli_spawn::start_cli_harness_session`, or `svc_start_sandboxed_cli_harness_session` when sandboxed). Existing `tool` and `claude-cli` sessions are unaffected.

### Session manager

`CliSessionManager` in `packages/tddy-daemon/src/cli_session_manager.rs` (re-exported as `claude_cli_session`) handles both `claude-cli` and `cursor-cli` PTY sessions. `launch_host_pty` registers a Cursor PTY with `PtyHandle.kind = "cursor-cli"`.

### Hook wiring

- `tddy_coding_agent_specs::slots::cursor::render_cursor_hooks_json` (`cursor-hooks-json`) — emits `.cursor/hooks.json` v1 (seven hook events), each naming the daemon socket. Every `/` in the file is escaped as `\/` (valid JSON): Cursor reads the file through a `parseJSONC` that strips `//` and `/*…*/` blind to string literals, so an unescaped `--daemon http://…` would fail the whole file and leave the session with no hooks at all.
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

When `managed_codebase = true`, the daemon seeds `changeset.yaml`, writes orchestration guidance to `.cursor/rules/tddy-managed-workflow.mdc` in the worktree (the definition's `rules_file`), hands `TDDY_SOCKET` to the `tddy-tools` MCP server and the session pointer (never the agent process), and exposes `subagent_new_session` / `prompt` / `cancel` via MCP. `specialized_agents` resolves to `TDDY_SUBAGENTS_JSON` / `TDDY_SUBAGENT` in the `tddy-tools` MCP server's env; the jailed agent's own process sees no `TDDY_*` variable.

### Workflow turns (`--agent cursor`)

A workflow turn is a `SpecHeadlessBackend` turn rendered from `cursor.yaml`'s `coding_agent.headless` section: `agent -p … --force --trust --approve-mcps` (`--plan` for a planning goal), the system prompt in front of the prompt, `tddy-tools --mcp` registered in `<worktree>/.cursor/mcp.json` — git-excluded while the turn runs and removed when it ends. The agent's environment carries no `TDDY_*` variable; its `tddy-tools` finds the session through the `.tddy/session.json` pointer the turn writes in the worktree (socket, repo and session directory, and the `TDDY_REMOTE_*` remote tool environment). See [tddy-agent-backend architecture](../../../packages/tddy-agent-backend/docs/architecture.md#backend-backend).

## Known follow-ups

- **Jail authentication:** a sandboxed `agent` cannot reach macOS Keychain — the host (`tddy-daemon` / `tddy-sandbox-app`) is an unsigned Rust binary with no `keychain-access-group` entitlement, so `securityd` rejects `SecItem*` calls with `errSecMissingEntitlement` (-34018) and the CLI hangs/times out ("Failed to store authentication tokens: Security command failed: Security process exited with code: 36"). `cursor.yaml`'s `files:` overlays set `CURSOR_SKIP_KEYCHAIN=1` + `CI=true` (Cursor's own CI convention) so the agent never probes Keychain. The caller must still supply `CURSOR_API_KEY` (User API key from the Cursor dashboard — admin keys are rejected) or pre-seed `~/.cursor/auth.json` (seeded once into the persistent home). True Keychain *access* from the jail would require codesigning the host with `keychain-access-group` and a TCC grant — recorded as a follow-up, not pursued; the current stance is fail-closed with `CURSOR_API_KEY`/seeded `auth.json`.
- **Jailed resume:** a sandboxed cursor-cli resume relaunches on the host PTY; a jailed resume is not implemented.
- **Replies are not in the transcript.** A cursor-cli session's ACP transcript (`ReadSessionTranscript`, the web's Activities view) holds its tool calls but never what Cursor said. The reply arrives on `afterAgentResponse` (`text`) and the prompt on `beforeSubmitPrompt` (`prompt`), but the transcript writer matches Claude's event names, `afterAgentResponse` is not installed, a report with no status change never reaches the daemon, and Cursor's chat id is not counted as the session's own conversation. Recorded in the backlog as 2026-11-24-cursor-reply-text-not-in-transcript.md.
- **The start-option model list lacks models the TUI offers.** `DescribeSessionStartOptions` lists Cursor's models from `agent --list-models`, which (Cursor Agent 2026.11.22) omits models the TUI's `/model` picker offers, such as GLM 5.3 Flash. `agent --model` accepts them all the same, and a session already on one keeps it across a resume. The gap is Cursor's; recorded in the backlog as 2026-11-24-cursor-model-list-lacks-tui-models.md.

## Out of scope (v1)

- **`WaitingForInput`** activity status (Cursor hooks have no permission-prompt equivalent — documented gap)
- TDD/bugfix workflow integration (no recipe)
- Switching a session's model from tddy after its start (a `/model` pick inside the TUI is followed — see [Resume](#resume-connect-signal-delete))
- Session chaining (`previous_session_id`)
- LiveKit presence for cursor-cli processes
- **`ConnectionScreen`** legacy inline start form third session type (CreateSessionPane is primary)

## References

- [Claude Code CLI session](claude-cli-session.md)
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md)
- [Telegram session control](telegram-session-control.md)
- [Cursor Hooks docs](https://cursor.com/docs/hooks.md)
- [Cursor CLI headless docs](https://cursor.com/docs/cli/headless)
