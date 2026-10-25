# Initial Discovery: Project-home agent waits for its project's notifications

**Changeset**: [2026-10-25-home-inbox-home-wait.md](./2026-10-25-home-inbox-home-wait.md)
**Date**: 2026-10-25
**Passes**: 4 (copied whole from the `#home-inbox` whole-work discovery, Explorations 1–4)
## Combined Conclusions

### Replan 2026-10-26: `master` moved, and the stack is rebased onto `#start-parity`

These points supersede anything below that contradicts them. The evidence is in the last Exploration.

- **Merged since pass 1.**
  - `#term-snapshot` #760–#763: `ScreenSnapshot` is at `tddy-task/src/terminal_capture.rs:391` on `master`.
  - `#coagents` #720–#727 and #776.
  - `#start-parity 1/9` remote-targets (#750).
- **New base.** The base is `#start-parity 9/9` (#758, `feature/start-parity/spawn-conversation`),
  the end of that line. Branching off a middle node such as #755 would fork a registered stack, and
  `gh stack` models only a line.
  - Cost: wave-1 nodes `1/8` and `2/8` need nothing from `#start-parity`, but now land after it.
  - `#start-parity` #751–#758 are not yet rebased onto `master`, so this base lacks `#coagents`. It
    gains them when that line is rebased.
- **`3/8` extends `#start-parity 6/9`'s `PendingQuestion`.** It no longer builds its own `Elicitation`
  record, `elicitations.json` or `ReportElicitation` RPC.
  - #755 records `{kind, header, question, options, allow_other}` in `<session dir>/pending-question.json`.
  - For claude-cli, #755 fills it only with the `Notification` hook's `message`
    (`hook_reports.rs`, `ReportSessionStatusRequest.message = 5`).
  - `3/8` adds the structured fields, the two hooks, and a keep rule.
  - The stack's ids are `request_id`, matching `#coagents`' `PermissionRequestView.request_id`. The old
    names (`Elicitation`, `elicitation_id`, `AwaitElicitationAnswer`, `elicitation_wait_secs`) are
    retired. They are now `PendingQuestion`, `request_id`, `AwaitQuestionAnswer` and `answer_wait_secs`.
- **`6/8` and `8/8` use `#coagents`' answering vocabulary.** That is `PermissionRequestView`,
  `PermissionAnswer`, `PermissionAnswering` (first answer wins) and `AnsweredBy` (gains `Orchestrator`)
  from `tddy-discovery/src/subagent/permission.rs`. The answer tool's fields mirror
  `AnswerAgentPermissionRequest`.
- **Every new target tool takes `daemon_instance_id`.** These are `ReadSessionTerminal`,
  `ClaimSessionTerminal`, `SendSessionKeys`, `AnswerSessionPermission`, `AnswerSessionQuestion`,
  `WaitProjectNotifications` and `ReadProjectNotifications`. #750's generic forward runs each on the
  target's host.
  - The hooks wait for an answer only while the orchestrator is on the child's own host. Cross-host
    presence is a TODO.
- **Who may answer.** The child's `orchestrator_session_id` must name the caller, whether it is a
  project-home session or a spawn-capable one (#758).
- **Paging.** `7/8` reuses #755's `FollowPage { items, next_cursor, has_more, clamped_to }` and its
  50/200 limits.
- **Shared registration.** Every `#start-parity` node adds tools to the same lists: the catalog,
  `is_project_home_allowed_tool`, the `read_only.rs` pin, `project_home_session_tools_acceptance.rs`,
  `exec_tool_admission.rs` and `project_home_prompt.md`. Rebases conflict there mechanically.

- **Base.** When Explorations 2–4 ran, `#term-snapshot` (#760–#763) was planning-only on its
  branches. By the time the root was cut (`room-wire` at `f4bd9652c`) its pass-2 contract commits had
  landed: `tddy_task::ScreenSnapshot` (`terminal_capture.rs:264`) and the
  `snapshot_and_subscribe` sequence-point test exist, with bodies still to be greened. The terminal
  nodes (`2/8`, `5/8`) need `#760` (and `#762` for jailed terminals) **green**; nothing else in this
  stack uses it.
- **Notifications.** `SessionNotification { session_id, os_user, label, kind, source, text, at_unix_ms }`
  carries final prose; three publishers; two fixed subscribers registered in
  `tddy-daemon/src/runtime/mount_host.rs` (Telegram's `OperatorChannelSubscriber`, the live-only
  `StreamSessionNotifications` relay). `DaemonRuntime` does not expose the bus — Tddy Desktop needs a
  `RuntimeOptions` hook. Telegram sends plain text, unchunked on this path, deduped per
  `(session_id, text)`. The web reads only `kind/sessionId/atUnixMs`.
- **Agent → daemon routes.** Ordinary claude-cli sessions have only the hook route
  (`tddy-tools session-hook`, hook token in `.session.yaml` and `.claude/settings.local.json`,
  fail-quiet, silent stdout); no `TDDY_REMOTE_*`. Project-home mains, split, workflow and sandbox
  sessions have the MCP exec route. `run_locally` interception in `tddy-daemon-rpc` is the place for
  daemon-answered project-home tools.
- **Elicitations.** Claude's `Notification` hook content (message, tool, input) is dropped;
  `PermissionRequest` and `PreToolUse` hooks are not installed; nothing writes
  `.session.yaml.pending_elicitation`. No daemon RPC holds a caller for a human answer; the
  presenter's toolcall `Ask`/`Approve` is the in-process prior art; `subagent_prompt`/`await` is the
  bounded-wait-then-redeem prior art.
- **Terminals.** Main terminal = `DaemonTerminalSessionStore::get_terminal(sid, "main")` (sandbox
  first), reachable only through `pub(crate) terminal_store()`. Leases are per session, claimed with
  `steal`, **never released**; jailed terminals skip the lease check (`requires_control() == false`);
  the room wire and the coder participant bypass leases. avt 0.18 gives visible text via
  `Vt::view()` + `Line::text()`, cursor via `Vt::cursor()`; `Vt::text()` is the wrong buffer on the
  alternate screen.
- **Tool timeouts.** LiveKit clamps `Await`/`Shell` to 20 s (`remote_block_arg_keys`); HTTP relay has
  no timeout; a blocking wait must stay ≤ 20 s and join the clamp list.
- **Catalog pins** a new project-home tool joins: `read_only.rs` test, `project_home_session_tools_acceptance.rs`,
  `exec_tool_admission.rs`, `project_home_prompt.md` (+ its test), feature docs.
- **Developer decisions (2026-10-25).** Base on `room-wire`, one registered line after #763;
  `tauri-plugin-notification`; `pulldown-cmark` → Telegram HTML; structured answers for permissions
  and questions (bounded hook wait, fallback to Claude's prompt); keystrokes only while the home agent
  holds the lease it claimed (a person reclaims by steal); notifications by a Superset-style
  `tddy-tools notify` CLI; a `RuntimeOptions` subscriber hook; pass 1 only until further notice.
- **Backlog/code issues.** No touched code issue is claimed by another PR. "Do not grow" constraints:
  `tddy-tools/src/server.rs`, `tddy-tool-engine/src/lib.rs`, `telegram_notifier.rs`,
  `telegram_session_control/*`, `connection_service.rs`, `SessionsDrawerScreen.tsx`.

## Exploration 1: parent pre-interview survey — 2026-10-25

**Agent**: parent Grep/Glob/Read
**Scope**: what exists today for notifications, elicitation signals, the home agent's tools, and
the `#term-snapshot` stack this work bases on.

### Sequence

1. Read `docs/ft/daemon/session-notifications.md` §§ What is notified … Related — the bus today.
2. Grep `notification` in `packages/tddy-desktop` (rs/json/toml) and `src-tauri/Cargo.toml`,
   `tauri.conf.json` — does Tddy Desktop show native notifications?
3. Grep `pending_elicitation\s*[:=]` across `packages` (non-test) — where the flag is set.
4. Grep `PermissionRequest|"Notification"` across `packages` (non-test) — which Claude hooks are
   installed.
5. Read `docs/ft/daemon/telegram-notifications.md` §§ Claude Code CLI session activity alerts …
   Implementation surface.
6. Read `packages/tddy-agent-backend/src/claude_hooks.rs:60-150` — hook events written into a
   claude-cli worktree.
7. `gh pr list --search "term-snapshot in:title"` and `git log origin/master..origin/feature/term-snapshot/*`
   — state of the base stack.
8. `git show origin/feature/term-snapshot/room-wire:docs/dev/1-WIP/2026-10-25-term-snapshot-host-snapshot.md`
   §§ Responsibility, Boundaries — what the base stack delivers.
9. Earlier in this session (dev-3.0 analysis): read `packages/tddy-tool-engine/src/project_home_tools.rs`,
   `packages/tddy-tool-engine/src/read_only.rs`, `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`,
   `packages/tddy-tools/src/exec_tool_admission.rs:85`, `packages/tddy-tools/src/server.rs:401-423,1520-1560`,
   `docs/ft/daemon/project-concept.md` §§ Project-home session … Git tools,
   `docs/ft/daemon/agent-session-status.md`, `project-home-auto-stop.md`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `notification` | `packages/tddy-desktop` | none — no native notification code or plugin |
| Grep | `pending_elicitation\s*[:=]` | `packages` | only `false` literals in constructors and test builders; `ListedSession.pending_elicitation` in `project_home_tools.rs:90` |
| Grep | `PermissionRequest\|"Notification"` | `packages` | `tddy-agent-backend/src/claude_hooks.rs:74,135` (`Notification` hook); ACP `RequestPermissionRequest` in `convert_acp.rs`, `backend/acp.rs`, `acp-stub` — no Claude `PermissionRequest` hook |

### Inspected files

#### `docs/ft/daemon/session-notifications.md`

**Why**: the notification model the stack extends.
**Excerpt**:

```text
| `ATTENTION_REQUIRED` | `WaitingForInput`, `Done` | ✓ (unchanged copy) | yellow |
| `ATTENTION_REQUIRED` | a presenter elicitation | ✓ *(via its own keyboard surface)* | yellow |
| `ACTIVITY` | `Started` / `Running` / `ExecutingTool`, an agent tool call, a workflow state change | — | blinking green |
...
One daemon-level ActivityService.StreamSessionNotifications subscription serves the whole drawer ...
It is live-only: a replayed backlog would raise indicators for turns that finished while the tab was closed.
The feed is scoped to its subscriber ... scoped to the caller's OS user
```

#### `docs/ft/daemon/telegram-notifications.md`

**Why**: the only operator channel; its copy is fixed.
**Excerpt**:

```text
- WaitingForInput: "🔔 Session <label>: Claude Code needs your input (permission, question, or your next prompt). ..."
- Done: "✅ Session <label>: Claude Code finished this turn. Attach to continue."
... the per-worktree `tddy-tools session-hook` calls [ReportSessionStatus] whenever Claude Code fires a
`Notification` hook event with `notification_type` of `permission_prompt`, `elicitation_dialog`, or `idle_prompt`.
Telegram reaches the daemon's core as its operator channel: one port
(`tddy_daemon_kernel::operator_channel::OperatorChannel`) ... `TelegramDaemonHooks` ... registered on the
session-notification bus, which `tddy_session_activity`'s `report_session_status` publishes to
```

#### `packages/tddy-agent-backend/src/claude_hooks.rs`

**Why**: which Claude hook events reach the daemon.
**Excerpt**:

```rust
let events = ["SessionStart", "UserPromptSubmit", "PostToolUse", "Notification", "Stop", "SessionEnd"];
let cmd = format!("{} session-hook --session {} --daemon {}{} --os-user {} --hook-token {} --event {}", ...);
```

#### `packages/tddy-tool-engine/src/project_home_tools.rs`

**Why**: the home agent's session tools and the port to the daemon.
**Excerpt**:

```rust
pub struct ListedSession { pub session_id, pub branch, pub worktree, pub is_active: bool,
    pub agent_status: String, pub pending_elicitation: bool }
#[async_trait] pub trait ProjectHomePort: Send + Sync {
    async fn list_sessions(&self, project_id: &str, home_session_id: &str) -> anyhow::Result<Vec<ListedSession>>;
    async fn start_session(...) -> anyhow::Result<String>;
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>>;
    async fn push_identity(&self, target: &SessionTarget) -> anyhow::Result<PushIdentity>;
}
pub fn project_home_offered_catalog() -> Vec<ToolDef> { tool_catalog().filter(is_project_home_allowed_tool)
    .chain(project_home_tool_defs()).chain(project_home_git_tool_defs()).collect() }
```

#### `packages/tddy-tool-engine/src/read_only.rs`

**Why**: one predicate decides advertised, pre-approved (`--allowedTools`) and executed tools.
**Excerpt**:

```rust
pub fn is_project_home_allowed_tool(name: &str) -> bool {
    is_read_only_tool(name) || crate::project_home_tools::is_project_home_tool(name)
        || crate::project_home_git_tools::is_project_home_git_tool(name) }
pub fn project_home_allowed_tools() -> Vec<String> { project_home_offered_catalog()...map(|t| format!("{MCP_TOOL_PREFIX}{}", t.name)) }
```

#### `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`

**Why**: where project-home tools are served (role gate, port impl, call log).
**Excerpt**:

```rust
struct DaemonProjectHome { launch: LaunchSessions, session_token, os_user, sessions_base, tddy_data_dir, identity }
async fn list_sessions(...) { self.launch.list_sessions_at_session_coordinate(...) ... .filter(project_id && != home) }
fn record_call(sessions_base, req, response) { tool_call_log::append_tool_call(&session_dir, &record) }
```

#### `origin/feature/term-snapshot/room-wire:docs/dev/1-WIP/2026-10-25-term-snapshot-host-snapshot.md`

**Why**: the base stack's surface a TUI-read tool consumes.
**Excerpt**:

```text
- `tddy_task::ScreenSnapshot { data, offset, rows, cols }`, `TerminalCapture::{snapshot, resize}`,
  the screen model fed by `TerminalCapture::append` (`avt 0.18`, incremental UTF-8) ...
- `TerminalSession::snapshot_and_subscribe() -> Result<(ScreenSnapshot, broadcast::Receiver<Bytes>), Status>`
- Boundaries: No mid-stream resync (2/4). No room wire (4/4). No jail sizing — Jailed terminals refuse
  SNAPSHOT with FAILED_PRECONDITION, and #term-snapshot 3/4 lifts that.
```

### Findings

- Notifications: two fixed kinds on a host-wide, OS-user-scoped, live-only bus; fixed copy; Telegram
  is the only operator channel (`OperatorChannel` port); the web drawer is a second subscriber.
- Tddy Desktop: no native notifications.
- claude-cli elicitations: only a status word from the `Notification` hook; no content; no
  `PermissionRequest` hook installed.
- Home agent: four session tools + seven git tools, all through one allow predicate; list is a
  synchronous pull; no wait, no events, no terminal access.
- `#term-snapshot` #760–#763 (all draft, in a yolo run; `resync` pinned in
  `.worktrees/feat-terminal-snapshot-on-reconnect`) delivers the avt screen model and snapshots for
  jailed and room-carried terminals; its tail has a reserved sweep node.

## Exploration 2: claude-cli elicitation capture and answer paths — 2026-10-25

**Agent**: Explore subagent
**Scope**: claude-cli hook installation, `tddy-tools session-hook`, `ReportSessionStatus`,
`pending_elicitation`, workflow-session elicitations and their answer surfaces, existing blocking
patterns. Ref `origin/feature/term-snapshot/room-wire` (`215fe17a8`); `git diff --stat HEAD <ref> -- packages`
is empty, so `packages/**` paths are identical on the ref and on `analyze-dev3-orchestrator`.

### Sequence

1. `git rev-parse`, `git diff --stat HEAD <ref>`, `git ls-tree -r <ref> | grep claude_hooks|session_hook|claude_cli_spawn|permission-mode|approval_prompt|code-issues` — drift + file map.
2. `git show <ref>:packages/tddy-agent-backend/src/claude_hooks.rs`.
3. `git grep 'settings\.local\.json|session-hook|permission-prompt-tool|approval_prompt|PermissionRequest|PreToolUse|AskUserQuestion'` over packages.
4. Read `tddy-agent-launch/src/claude_cli_spawn_steps.rs`, `claude_cli_spawn.rs:39-60,185-300`, `svc_spawn_split_agent.rs:225-270`, `svc_start_sandboxed_claude_cli_session.rs` (grep), `project_home_launch.rs`, `chat.rs:20-60`.
5. Read `tddy-session-split/src/service_util.rs:290-330`, `tddy-cli-sessions/src/cli_session_manager/{argv.rs,launch.rs:160-240}`, `tddy-agent-backend/src/claude_argv.rs:1-120`.
6. Read `tddy-tools/src/session_hook.rs` (whole), `tddy-tools/src/server.rs:55-80,615-860,1850-1870`.
7. Read `tddy-toolcall/src/toolcall/{mod.rs:100-280,listener.rs:1-60,560-760,client.rs:1-90}`, `tddy-tools/src/cli/relay.rs:40-80`, `tddy-cli-sessions/src/session_toolcall.rs:1-40`.
8. Read `tddy-presenter/src/presenter/{presenter_impl/questions.rs:1-260,state.rs,workflow_runner.rs:225-260}`, `tddy-workflow/src/questions.rs`.
9. grep `pending_elicitation`; read `tddy-session-activity/src/pending_elicitation.rs`, `tddy-changeset/src/{session_metadata.rs,session_activity.rs:1-160}`, `tddy-coder/src/session_participant/metadata_publisher.rs:60-160`, `tddy-agent-launch/src/hooks_and_urls.rs:60-90`.
10. Read `tddy-session-activity/src/{service/hook_reports.rs,service.rs:85-125,session_notifications.rs:220-275,presenter_intent_client.rs,presenter_observer_task.rs:1-40}`, `tddy-service/proto/activity.proto`, `presenter_intent.proto`, `remote.proto`, `tddy-service/src/presenter_intent_service.rs:1-120`, `tddy-daemon/tests/local_token_uds.rs:700-760`, `tddy-daemon-kernel/src/daemon_hook_urls.rs:30-60`.
11. Read `tddy-telegram-control/src/telegram_session_control/elicitation.rs:1-140`; `docs/ft/daemon/{claude-cli-permission-mode.md,claude-cli-session.md}`; `tddy-coder/src/acp_host.rs:85-130`.
12. Loop over `packages/<p>/docs/code-issues/*.md` for touched packages + `grep '**Claimed by'`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `PermissionRequest\|PreToolUse\|AskUserQuestion\|approval_prompt\|permission-prompt-tool` | packages | `backend/claude.rs:56,128,346`; `sandbox-recipes/claude_cli.rs:164,400`; `project_home_launch.rs:109`; `session_toolcall.rs:12`; `session_activity.rs:267` (PreToolUse → no status); no Claude `PermissionRequest` hook anywhere |
| Grep | `fn write_claude_hooks_settings` | packages | `tddy-session-split/src/service_util.rs:304` |
| Grep | `HookCommandParams {` (non-test) | packages | `svc_spawn_split_agent.rs:249`, `claude_cli_spawn_steps.rs:174`, `chat.rs:30` (cursor) |
| Grep | `hook_token` | agent-launch, session-split, sandbox | claude `claude_cli_spawn_steps.rs:171`; split `svc_spawn_split_agent.rs:246`; cursor `cursor_cli_spawn.rs:228`; **none on the sandboxed claude-cli path** |
| Grep | `pending_elicitation` (non-test) | packages | readers only + `metadata_publisher.rs:130` (LiveKit participant metadata) — no writer of `.session.yaml.pending_elicitation = true` |
| Grep | `long.poll\|grace period\|responseId` | packages | `tddy-tools/src/server.rs:1857-2130` (subagent prompt/await) |
| Grep | `**Claimed by` | touched packages' code-issues | none |

### Inspected files

#### `packages/tddy-agent-backend/src/claude_hooks.rs`

```rust
let events = ["SessionStart","UserPromptSubmit","PostToolUse","Notification","Stop","SessionEnd"];
let cmd = format!("{} session-hook --session {} --daemon {}{} --os-user {} --hook-token {} --event {}", ...);
serde_json::json!([{ "matcher": "", "hooks": [{"type": "command", "command": cmd}] }])
// test :122 asserts hooks.len() == 6; no per-hook `timeout`
```

#### `packages/tddy-session-split/src/service_util.rs:304-319`

`write_claude_hooks_settings(cwd, params)` writes `<cwd>/.claude/settings.local.json` whole and
atomically (only a `hooks` key), warn-and-continue on failure.

#### `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs`

- `:150-184` `install_claude_cli_hooks`: `daemon_url = claude_hook_daemon_url(config)`,
  `daemon_socket = embedded_host_agent_tool_socket(tddy_data_dir)`, `hook_token = Uuid::new_v4()`.
- `:278-286` `role_allowed_tools_args`: project-home → `--allowedTools` per `project_home_allowed_tools()`.
- `claude_cli_spawn.rs:190` installs hooks unconditionally (project-home mains included);
  `:271-292` `.session.yaml` gets `hook_token`. Resume (`svc_resume_claude_cli_session.rs`) rewrites no hooks.
- Sandboxed claude-cli (`svc_start_sandboxed_claude_cli_session.rs:339-374`): **no hooks**;
  `--permission-mode` or `--dangerously-skip-permissions`.

#### `packages/tddy-agent-launch/src/project_home_launch.rs:105-150`

```text
--allowedTools (project-home set), --disallowedTools NATIVE_FILESYSTEM_TOOLS,
--append-system-prompt-file project-home-prompt.md, tddy_tools_only_mcp_args(...),
env colocated_jail_tool_env(local_daemon_hook_url, session_id, agent_session_token, embedded socket)
// :109 "No `--permission-prompt-tool`: a main is an interactive terminal, so a call outside its
//       pre-approved set prompts its operator there."
```

#### `packages/tddy-agent-backend/src/claude_argv.rs:15-41`

```rust
if dangerously_skip_permissions { argv.push("--dangerously-skip-permissions") }
else { argv.push("--permission-mode"); argv.push(permission_mode) }   // default "auto"
```

#### `packages/tddy-tools/src/session_hook.rs`

- Args `--session --daemon --daemon-socket? --os-user --hook-token --event`; reads all stdin.
- `report_agent_activity` (Pre/PostToolUse) then `report_session_status`; `REPORT_TIMEOUT = 2s`.
- Transport: UDS `call_unary("activity.ActivityService", ...)` or `POST {daemon}/rpc/activity.ActivityService/{method}`.
- **Always exit 0, never writes stdout** — cannot return a hook decision (`hookSpecificOutput`,
  `permissionDecision`, `updatedInput`). Contract documented at `docs/ft/daemon/claude-cli-session.md:363`.

#### `packages/tddy-changeset/src/session_activity.rs`

```rust
"Notification" => match notification_type { Some("permission_prompt") | Some("elicitation_dialog")
    | Some("idle_prompt") => Some(WaitingForInput), _ => None },
pub struct HookEvent { hook_event_name, session_id, cwd, notification_type }  // message/tool_* dropped
```

#### `packages/tddy-session-activity/src/service/hook_reports.rs:25-83`

`report_session_status_impl`: wire check → `hook_session_dir` → metadata → `session_type ∈ {claude-cli,
cursor-cli}` → `hook_token` match (else `permission_denied`) → `update_activity_status` →
`publish_for_session(notification_for_activity_status)`. Never touches `pending_elicitation`.
`report_agent_activity_impl` (`:86-214`) pairs Pre/Post rows in `agent-activity.jsonl`.

#### `pending_elicitation`

- Read: `tddy-session-activity/src/pending_elicitation.rs:9-29` (`.session.yaml`) →
  `session_list_enrichment.rs:299` → `SessionEntry.pending_elicitation` (`session.proto:289`, field 14)
  → `ListedSession.pending_elicitation`.
- Written true by **nothing** in `.session.yaml`. Only LiveKit participant metadata
  (`metadata_publisher.rs:120-131`: Select/MultiSelect/TextInput/FeatureInput). claude-cli leaves it
  empty by design (`hooks_and_urls.rs:62-75`). No clear path.

#### Workflow elicitations (prior art)

- `tddy-presenter/src/presenter/state.rs:36-65` `AppMode::{FeatureInput, Running, DocumentReview,
  MarkdownViewer, Select{question,..}, MultiSelect, TextInput{prompt}, Done, ErrorRecovery}`.
- `tddy-workflow/src/questions.rs` `ClarificationQuestion { header, question, options[{label, description}], multi_select, allow_other }`.
- `presenter_impl/questions.rs:99-209`: `ToolCallRequest::Ask{questions, response_tx}`,
  `Approve{tool_name, input, response_tx}` → synthetic Allow/Deny Select; `:36-92` answers via oneshot.
- `tddy-toolcall/src/toolcall/listener.rs:660-708` `await_presenter_response` — **no timeout**.
- Answer surfaces: child-local `PresenterIntent` gRPC (`AnswerClarificationSelect{option_index}`,
  `MultiSelect{indices, other}`, `Text{text}`, document approve/reject/refine;
  `presenter_intent_service.rs:28-120`), used by Telegram (`elicitation.rs:68-141`) via
  `presenter_intent_client::*_localhost(port)`; `TddyRemote.Stream` `ClientMessage` (web).

#### `packages/tddy-tools/src/server.rs`

- `:59-66,733-738` `approval_prompt(tool_name, input)` → `decide` (`:615-654`) → `relay_approve`
  (`:656-710`): legacy NDJSON over `TDDY_SOCKET`, 60 s deadline; the listener now speaks `tddy-rpc`
  framing, so this relay probably always denies (not run). Recorded in
  `docs/dev/todo/2026-09-10-the-permission-engines-ndjson-unix-socket-protocol-survives.md`.
- `:1855-2130` `subagent_prompt` (`graceMs` → `{responseId, pending:true}`) + `subagent_await` —
  the only bounded-wait-then-redeem pattern (in tddy-tools, not a daemon RPC).
- `tddy-cli-sessions/src/session_toolcall.rs`: a managed claude-cli's per-session listener serves
  `transition` only; "ask/approve for claude-cli go through approval_prompt + PTY"; nothing drains Ask/Approve.

### Findings

- No daemon-side "hold the caller until a human answers" RPC exists; human waits live in the
  `tddy-coder` presenter process.
- Adding `PermissionRequest` and `PreToolUse(AskUserQuestion)` hooks touches
  `build_claude_hooks_settings` (6-event test), needs per-hook `timeout`, and needs a new
  `session-hook` mode that **blocks and prints decision JSON** — contradicting today's documented
  fail-quiet contract, so it must be a separate subcommand/event, not a change of the existing one.
- `HookEvent` must carry `message`, `tool_name`, `tool_input` for content capture.
- `pending_elicitation` has no writer — the elicitation store is new; its existing field can become
  real (written on capture, cleared on answer/PostToolUse/Stop).
- Sandboxed claude-cli has no hooks — out of scope for this stack (record as a deferral).
- Code issues in touched packages: none claimed. Relevant oversized/complex records:
  `tddy-tools/oversized-file-server`, `tddy-toolcall/oversized-file-listener`,
  `tddy-telegram-control/oversized-file-telegram-notifier`, `tddy-tool-engine/oversized-file-lib`,
  `tddy-presenter/complexity-presenter-impl-questions-send-clarification-answers`.

## Exploration 3: terminal surfaces, control lease and project-home tool plumbing — 2026-10-25

**Agent**: Explore subagent
**Scope**: screen model from `#term-snapshot`, avt 0.18 API, finding a session's main terminal,
terminal input and the control lease, project-home tool plumbing, call timeouts, catalog pins.

### Sequence

1. `git log/merge-base/diff --stat` of `origin/feature/term-snapshot/room-wire` vs HEAD; `ls ~/.cargo/registry/src/*/ | grep avt` (found `avt-0.18.0`).
2. `git show --stat` of the four `#term-snapshot` commits; `git grep ScreenSnapshot|snapshot_and_subscribe|\bavt\b` (empty); `git grep vt100|vte|alacritty` in `*.toml`.
3. Read `packages/tddy-task/src/terminal_capture.rs`, `task.rs:196-216`.
4. Read the four `#term-snapshot` changesets and `…host-snapshot-initial-discovery.md`.
5. Read avt `lib.rs`, `vt.rs:1-82`, `terminal/cursor.rs`, `terminal.rs:120-132,618-656`, `line.rs:310-356`, `buffer.rs` (`text`/`view`/`lines`/`trim_scrollback`).
6. Read `packages/tddy-terminal-rpc/src/{session.rs,service.rs:60-260,service/ports.rs:1-80,bridge.rs:415-520}`; `docs/ft/daemon/terminal-sessions.md:1-180,377-410`.
7. Read `tddy-cli-sessions/src/cli_session_manager/{control_lease.rs,pty_handle.rs:20-125,terminals.rs:60-100,pty_terminal_service.rs:1-60}`, `cli_session_manager.rs:30-50`; grep lease removal (`control.write()|remove|release|clear`).
8. Read `tddy-session-lifecycle/src/{connection_service/svc_terminal_ports.rs:1-160,terminal_session_adapter.rs}`; `tddy-daemon-sandbox/src/sandbox_session.rs:30-58,425-445`.
9. Read `tddy-tool-engine/src/{project_home_tools.rs,read_only.rs,project_home_git_tools.rs:1-80,catalog.rs,lib.rs:625-770}`.
10. Read `tddy-daemon-rpc/src/{exec_tool.rs,exec_tool/project_home.rs,exec_tool/role_guard.rs,exec_tool/ports.rs:25-200}`.
11. Read `tddy-session-tool-client/src/{lib.rs:42-430,rpc_transports.rs:110-170}`; grep `MCP_TOOL_TIMEOUT` (none); `tddy-tools/src/server.rs:670-690`.
12. Grep every pin of `ListProjectSessions|DiffSessionWorktree|CleanSessionWorktree`; read `project_home_session_tools_acceptance.rs:490-540`, `project_home_git_tools_acceptance.rs:320-350`, `tddy-tools/src/exec_tool_admission.rs:1-60,115-170`, `ensure_project_home_session_acceptance.rs:465-485`, `tddy-worktree-service/src/session_target.rs:14-120`, `tddy-agent-launch/src/project_home_prompt.md`.
13. Listed code issues for 11 candidate packages with `Claimed by` lines.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `ScreenSnapshot\|snapshot_and_subscribe\|\bavt\b` | `*.rs *.toml *.proto` on the ref | **none** — `#term-snapshot` is docs-only (pass 1) at `215fe17a8` |
| Grep | `vt100\|vte\|alacritty_terminal\|wezterm-term` | `*.toml` | only `tddy-tui-testkit` dev-dep `vt100 0.15` |
| Grep | `impl TerminalSessionStore for\|fn get_terminal` | packages | `cli-sessions/terminals.rs:74`, `coder/terminal_manager.rs:183`, `session-lifecycle/terminal_session_adapter.rs:193` |
| Grep | `control.write()\|control.remove\|release_control\|clear_control` | `packages/*/src` | only `control_lease.rs:22` (claim) — **no release path** |
| Grep | `REMOTE_ENGINE_DEFAULT_BLOCK_MS` | packages | `tddy-session-tool-client/src/lib.rs:128` (+ mirrors in tool-engine `lib.rs:473,634,707`) |
| Grep | `MCP_TOOL_TIMEOUT\|MCP_TIMEOUT` | packages, docs | none |

### Inspected files

#### `packages/tddy-task/src/terminal_capture.rs` (ref = working tree)

64 KiB raw ring (`CAPTURE_LIMIT_BYTES`) + mouse-mode sniffer; `append`, `replay`, `replay_last/before/from`.
No grid, size, cursor or text. (`terminal-sessions.md:16` says 512 KB — stale.)

#### `#term-snapshot` host-snapshot changeset (plan only)

```text
TerminalCapture holds an avt::Vt sized to the PTY ... snapshot() returns RIS + Vt::dump() + the input
modes from the sniffer, with offset = end_offset.
TaskChannel::write: let mut cap = lock; cap.append(&data); let _ = output_tx.send(data); drop(cap)
TerminalSession::snapshot_and_subscribe() — default impl; adapters that cannot size a model refuse.
```

`ScreenSnapshot { data, offset, rows, cols }` is bytes — **no text lines, no cursor**. A text grid
needs a new accessor on `TerminalCapture` over the `Vt`.

#### avt 0.18.0 (`~/.cargo/registry/src/127.0.0.1-c6cce136a2919305/avt-0.18.0/src/`)

```rust
Vt::new(cols, rows); builder().size().scrollback_limit().build(); feed_str(&str) -> Changes;
size() -> (cols, rows); resize(cols, rows); view() -> impl Iterator<Item=&Line>; text() -> Vec<String>;
cursor() -> Cursor  // pub struct Cursor { pub col: usize, pub row: usize, pub visible: bool }
Line::text() // untrimmed
```

- `Vt::text()` returns the **primary** buffer + scrollback even on the alt screen — use
  `view().map(|l| l.text().trim_end())` for the visible active screen.
- Active buffer type is not exposed by `Vt` (private `terminal` field).
- Scrollback GC only in `feed_str`/`resize`; a `None` limit never trims.

#### `packages/tddy-terminal-rpc/src/session.rs`

`TerminalSession { capture(), subscribe_stdout(), subscribe_pty_done(), subscribe_acked_offset(),
resizable(), requires_control(), resize(rows, cols), send_input(data, input_offset), trigger_redraw() }`;
`TerminalSessionStore::get_terminal(session_id, terminal_id)`.

#### `packages/tddy-session-lifecycle/src/terminal_session_adapter.rs`

```rust
// get_terminal(:193-212): sandbox registry first; only "main"; else manager.get_terminal(...)
SandboxTerminalSession { requires_control() -> false, resizable() -> false, send_input = stdin_tx.send }
DaemonTerminalSession { handle: Arc<PtyHandle> }  // send_input = strip_resize + stdin_tx.send + ack
```

`svc_terminal_ports.rs:35` `terminal_store()` is **`pub(crate)`**; `:71` `terminal_session_service()` public;
`:99-131` `CliManagerTerminalControl` (claim/verify/holder_screen_id/subscribe).

#### `packages/tddy-cli-sessions/src/cli_session_manager/control_lease.rs`

```rust
pub async fn verify_control(&self, session_id: &str, control_token: &str) -> bool {
    match control.get(session_id) { None => true, Some(lease) => lease.control_token == control_token } }
```

Per-session `HashMap<session_id, ControlLeaseInfo{control_token, holder_screen_id}>`; **never
removed** (no release, no clear on delete — contradicts the comment at `svc_terminal_ports.rs:97`;
"no heartbeat auto-release" is a stated non-goal in `terminal-sessions.md`). Once any screen claims,
`current_control(sid).is_some()` stays true for the daemon's lifetime.

#### `packages/tddy-terminal-rpc/src/service.rs`

`:87-95` `require_control` → `failed_precondition("terminal controlled by another screen")`;
`:105-123` `controlled_terminal` checks the lease only when `requires_control()`; sandboxed terminals
therefore bypass it. The room wire (`PtyTerminalService`) and the coder's `verify` (always true)
bypass leases entirely.

#### `packages/tddy-daemon-rpc/src/exec_tool.rs`, `exec_tool/project_home.rs`, `exec_tool/ports.rs`

- `ExecToolRpcHandler` holds no terminal store or control port today (`from_host` `:46-58`).
- `run_locally` `:43-79` predicate (`:49`) must include any new tool family; `project_home_caller`
  `:142-162` is the role gate; `role_guard.rs` refuses non-allowed tools.
- `stream_execute_tool` frames a completed result only — no progress/keepalive.

#### `packages/tddy-tool-engine/src/lib.rs:630-765`

Shell `block_until_ms` default 30 000, `0` → detached job; `Await {job_id, timeout_ms}` default 30 000.

#### `packages/tddy-session-tool-client/src/lib.rs`

```rust
const MAX_REMOTE_BLOCK_MS: u64 = 20_000;           // :107-120; forwarded-stream idle deadline is 30 s
const REMOTE_ENGINE_DEFAULT_BLOCK_MS: u64 = 30_000; // :128
fn remote_block_arg_keys(tool) // :147-153 — only "Await" and "Shell"; clamp applied on LiveKit only
```

`rpc_transports.rs:125-163`: HTTP relay `reqwest::Client::new()` with **no timeout**.

#### Catalog pins a new project-home tool must join

1. `packages/tddy-tool-engine/src/read_only.rs:87-114` (literal 16 MCP names).
2. `packages/tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs:490-519` (sorted 16).
3. `packages/tddy-tools/src/exec_tool_admission.rs:115-144` (+ `project_home_remote_tool_defs()` `:59`).
4. Text: `project_home_tools.rs` docs, `packages/tddy-agent-launch/src/project_home_prompt.md` (+
   `claude_cli_spawn_steps_tests.rs:395-398`), `tddy-tool-engine/README.md`,
   `docs/ft/daemon/project-concept.md` § Project-home tools, `svc_list_exec_tools_wiring.rs:99,114`.

### Findings

- `#term-snapshot` is **planning-only on its branches** (pass 1, docs). Every terminal node of this
  stack compiles against a surface that does not exist yet; its pass 2 must land first.
- A text grid is feasible from avt's public API; it is a **new** `TerminalCapture` accessor
  (`text_grid()`), not part of `#760`'s `ScreenSnapshot`.
- Main terminal: `DaemonTerminalSessionStore::get_terminal(sid, "main")` (sandbox first), reachable
  only through a `pub(crate)` accessor today.
- Lease: "a human holds the lease" = `holder_screen_id(sid).is_some()`, but leases are never
  released, so a naive check refuses forever after the first browser attach. **Needs a decision.**
- A blocking wait must cap itself under the LiveKit 20 s clamp (or join `remote_block_arg_keys`).
- Code issues: none claimed. Relevant: `tddy-task/complexity-terminal-capture-feed`,
  `tddy-terminal-rpc/complexity-bridge-open-replay-ack-live`, `tddy-tool-engine/oversized-file-lib`,
  `tddy-daemon-rpc/complexity-exec-tool-ports-stream-execute-tool`,
  `tddy-session-lifecycle/oversized-file-connection-service` (525 > 500 — touched if `terminal_store`
  is widened), `tddy-tools/oversized-file-server`.

## Exploration 4: notification bus, Telegram rendering, agent→daemon routes, Tddy Desktop — 2026-10-25

**Agent**: Explore subagent
**Scope**: bus types/publishers/subscribers, `StreamSessionNotifications`, Telegram rendering and
routing, the web consumer, how an agent tool could publish, Tddy Desktop hosting/plugins/navigation.
All paths identical on the ref and the working tree (`git diff --stat HEAD <ref>` empty).

### Sequence

1. `git ls-tree` of `tddy-session-activity`, `tddy-telegram-control`, `tddy-desktop/src-tauri`, `docs/ft/desktop`.
2. Read `tddy-session-activity/{README.md,docs/session-notifications.md,src/session_notifications.rs,src/session_notification_publishing.rs,src/session_notification_subscribers.rs,src/service.rs:1-140,src/service/{hook_reports.rs,ports.rs,rpc.rs:1-120},src/streams.rs:60-100}`.
3. Read `tddy-daemon-kernel/src/{session_notification.rs,operator_channel.rs,config.rs:285-291,agent_tool_socket.rs}`.
4. Read `tddy-service/proto/activity.proto:1-72,235-332`, `exec_tools.proto:1-110`.
5. Read `tddy-daemon/src/runtime/{mount_host.rs:1-80,180-220,mount.rs:20-45,telegram_wiring.rs}`, `runtime.rs:55-180`, `agent_tool_socket.rs:38-80`.
6. Read `tddy-telegram-control/src/{telegram_session_subscriber.rs,telegram_notifier.rs:25-60,415-450}`, `telegram_session_control/callbacks.rs:659`, `tddy-telegram/src/sender.rs`; grep `parse_mode|MarkdownV2|4096`.
7. Read `tddy-web/src/rpc/{useSessionNotifications.ts,daemonTransportFlavour.ts}`, `routing/appLocation.ts`.
8. Read `tddy-tools/src/{session_hook.rs:1-200,server.rs:920-950,1590-1700}`, `tddy-session-tool-client/src/lib.rs:1-330`, `tddy-agent-backend/src/backend/mod.rs:350-425`, `tddy-agent-launch/src/{project_home_launch.rs:1-240,claude_cli_spawn.rs:180-280,claude_cli_spawn_steps.rs:150-185}`, `tddy-tool-engine/src/{catalog.rs,project_home_tools.rs:1-200,exec_tool_service.rs}`, `tddy-daemon-rpc/src/exec_tool{.rs,/project_home.rs:1-135,/role_guard.rs,/ports.rs:25-100}`, `tddy-session-agents/src/exec_tool_caller.rs`.
9. Read `tddy-desktop/src-tauri/{Cargo.toml,tauri.conf.json,capabilities/default.json,build.rs,src/main.rs,src/lib.rs,src/ipc.rs}`, `docs/ft/desktop/tddy-desktop-tauri.md`; grep `notification|deep.link|set_focus|tray`.
10. Code-issues loop for touched packages; `git grep 'Claimed by:'`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `SessionNotificationBus\|OperatorChannelSubscriber\|SessionNotificationStreamSubscriber` | packages (excl. session-activity) | `tddy-daemon/src/runtime/mount_host.rs:12-17,44-51` — the only registration site |
| Grep | `parse_mode\|MarkdownV2\|4096\|send_message` | telegram crates | **no parse_mode** anywhere; `telegram_notifier.rs:37` 4096; `sender.rs:50-60` |
| Grep | `streamSessionNotifications` | `tddy-web/src` | `useSessionNotifications.ts:47` only |
| Grep | `notification\|deep.link\|set_focus\|unminimize\|tray` | `tddy-desktop` | none relevant |
| Grep | `new Notification\|requestPermission\|serviceWorker` | `tddy-web/src` | none |
| Grep | `Claimed by:` | repo | only untouched packages |

### Inspected files

#### `packages/tddy-daemon-kernel/src/session_notification.rs`

```rust
pub enum SessionNotificationKind { Activity, AttentionRequired }
pub enum SessionNotificationSource { ActivityStatus, AgentToolCall, Presenter }
pub struct SessionNotification { session_id, os_user /* server-side only */, label, kind, source,
    text /* "Telegram sends it verbatim; the web shows it as the dot's tooltip" */, at_unix_ms }
```

No title, body, level, sound or link.

#### `packages/tddy-daemon-kernel/src/operator_channel.rs`

`trait OperatorChannel { id(), follows_presenter_events(), on_presenter_event(session_id, &ServerMessage),
wants_notification(&SessionNotification) -> bool, on_notification(&SessionNotification) -> Result }`;
`NoOperatorChannel`; `awaits_operator(&ModeChanged)`.

#### `packages/tddy-session-activity/src/session_notifications.rs`

`SessionNotificationSubscriber { name, wants, deliver, client_relay }`; `SessionNotificationBus` is a
consuming builder (`with_subscriber`), `publish` sequential, errors logged and swallowed;
`OperatorChannelSubscriber`; builders `notification_for_activity_status` (fixed copy),
`notification_for_agent_tool_call`, `notification_for_presenter_event`; `LastDeliveredPerSession`.

#### `packages/tddy-session-activity/src/session_notification_subscribers.rs`

`NOTIFICATION_STREAM_CAPACITY = 256`; `SessionNotificationStreamSubscriber` — live-only broadcast.

#### `packages/tddy-session-activity/src/service/hook_reports.rs:25-83`, `service/rpc.rs:91-114`, `streams.rs:69-96`

Publish owner = hook-token-validated `req.os_user`. `StreamSessionNotifications`: token → os_user →
`bus.subscribe_clients()` → `relay_session_notifications` forwards on exact non-empty os_user match.

#### `packages/tddy-service/proto/activity.proto`

```proto
message ReportSessionStatusRequest { string session_id = 1; string hook_token = 2; string os_user = 3; string status = 4; }
message SessionNotificationEvent { string session_id = 1; string label = 2; SessionNotificationKind kind = 3;
  SessionNotificationSource source = 4; string text = 5; int64 at_unix_ms = 6; }
enum SessionNotificationKind { UNSPECIFIED = 0; ACTIVITY = 1; ATTENTION_REQUIRED = 2; }
enum SessionNotificationSource { UNSPECIFIED = 0; ACTIVITY_STATUS = 1; AGENT_TOOL_CALL = 2; PRESENTER = 3; }
```

#### `packages/tddy-daemon/src/runtime/mount_host.rs:43-53`

```rust
SessionNotificationBus::new()
    .with_subscriber(OperatorChannelSubscriber::new(self.operator_channel()))
    .with_subscriber(SessionNotificationStreamSubscriber::new())
```

Fixed list, no extension point; `DaemonRuntime` (`runtime.rs:161-180`) does not expose the bus.

#### `packages/tddy-telegram-control/src/telegram_session_subscriber.rs`

`wants_notification` = `enabled && kind == AttentionRequired && source == ActivityStatus`;
`recipients_of` tracked-first then `chat_ids`, nothing when unreadable; `send_to_every_chat` sends
`notification.text` verbatim, **no chunking**; dedupe on `(session_id, text)`.

#### `packages/tddy-telegram/src/sender.rs`

`bot.send_message(chat_id, text)` — **no `parse_mode`** (plain text); `TelegramSender { send_message,
send_message_with_keyboard }` — callback buttons only, no URL buttons. `chunk_telegram_text`
(`telegram_session_control/callbacks.rs:659`, 4096 UTF-8) used only for presenter bodies.

#### `packages/tddy-web/src/rpc/useSessionNotifications.ts:32-76`

Reads only `kind`, `sessionId`, `atUnixMs`; ignores unknown kinds (wire-compatible). Session route
shape: `#/sessions/<id>?host=<daemon_instance_id>` (`routing/appLocation.ts`).

#### Agent → daemon routes

- (a) Hook route: `tddy-tools session-hook` → `ActivityService.Report*`, auth `(os_user, session_id,
  hook_token)`; hook token exists only for claude-cli/cursor-cli (in `.claude/settings.local.json`
  and `.session.yaml`); fail-quiet, 2 s; not an MCP tool.
- (b) MCP exec-tool route: `tddy-tools --mcp` → `dispatch_session_tool` → `ExecToolService.ExecuteTool`
  (`session_token`, `session_id`, `daemon_instance_id`); transport from `TDDY_SANDBOX_TOOL_IPC` /
  LiveKit set / `TDDY_REMOTE_SESSION_ID`+`TDDY_REMOTE_DAEMON_SOCKET|URL`. `run_locally` interception
  (`project_home.rs:43-79`) is the precedent for daemon-answered tools. **An ordinary claude-cli
  session has no `TDDY_REMOTE_*`** (`claude_cli_spawn.rs`) — only project-home mains, split,
  workflow/tddy-coder and sandbox sessions do.
- (c) Per-session `TDDY_SOCKET` toolcall relay — managed-recipe sessions only.
- The embedded agent-tool socket serves the **whole** roster (`tddy-daemon/src/agent_tool_socket.rs:38-80`).

#### `packages/tddy-desktop/src-tauri/`

- `Cargo.toml`: `tauri = "2"`, `tauri-plugin-opener = "2"` (+ optional wdio); **no notification,
  deep-link or single-instance plugin**. `tauri.conf.json`: no `plugins`, `windows: []`, strict CSP.
  `capabilities/default.json`: `core:default` + three `tddy_rpc_*` commands. `build.rs` lists commands.
- `src/lib.rs`: `start_daemon` → `runtime::build(config, RuntimeOptions::for_embedded(..))` in-process;
  `MAIN_WINDOW_LABEL = "main"`; the only navigation precedent is `window.navigate(route)` in
  `complete_sign_in_when_the_browser_comes_back` (`:162-191`); no `set_focus`/`emit`/event channel.
- `docs/ft/desktop/tddy-desktop-tauri.md`: macOS + Linux; nothing on notifications.

### Findings

- The bus carries operator **prose**, not structured content; every subscriber sends `text`.
- Telegram is plain text, unchunked on the notification path; a markdown body needs a parse mode
  (HTML is the safe one) and chunking.
- No route lets **every** agent send a notification: the hook route reaches claude-cli/cursor-cli
  only and is not a tool; the MCP route reaches sessions with `TDDY_REMOTE_*` only.
- Desktop cannot reach the bus: the subscriber list is fixed in `mount_host.rs`, and `DaemonRuntime`
  exposes no bus — an extension point on `RuntimeOptions` is needed.
- Web reads no notification text; new fields/kinds are wire-compatible.
- `tddy-web` `SessionsDrawerScreen.tsx` is 504 lines (code issue) — avoid growing it.
- Not checked: a daemon config field for a public web base URL (for links in Telegram).

## Exploration 5: replan over the moved `master` and the `#start-parity` line — 2026-10-26

### Sequence

1. `gh pr list --state all` found these merged on `master`: #720–#727 and #776 (`#coagents`), #760–#763
   (`#term-snapshot`) and #750 (`#start-parity 1/9`).
2. `#start-parity` #751–#756 are ready for review and #757/#758 are drafts. A yolo run drives each in
   its own worktree (`.worktrees/yolo-start-parity-*`).
   - `merge-base --is-ancestor 857351c5d origin/feature/start-parity/spawn-conversation` says no: that
     line predates `#coagents` on `master`.
3. Read the code #755 (`feature/start-parity/session-follow`) landed for pending questions.
4. Read `#coagents`' permission vocabulary on `origin/master`.
5. Read #750's target-host argument on `origin/master`.
6. Checked that every existing path this stack's changesets name still exists on #758. All do.
   `claude_hooks.rs` is in `tddy-agent-backend`, as the docs already say.

### Inspected files

#### `packages/tddy-changeset/src/pending_question.rs` (#755)

- `PENDING_QUESTION_FILE = "pending-question.json"`.
- `PendingQuestion { kind: select|multi_select|text|permission, header, question, options: Vec<String>, allow_other }`.
- `write_pending_question` writes a temp file and renames it; `clear_pending_question` and
  `read_pending_question` read and clear it.
- The module doc says: "`.session.yaml`'s `pending_elicitation` says *that* a session is waiting;
  this file says *on what*. Two writers…"

#### `packages/tddy-session-activity/src/service/hook_reports.rs` (#755)

- If `status == WaitingForInput` and the `message` is non-empty, it writes
  `PendingQuestion { kind: "permission", question: message }`.
- Every other status calls `clear_pending_question`.
- `PostToolUse` maps to `ExecutingTool`, `UserPromptSubmit` to `Running`, `Stop` to `Done` and
  `SessionEnd` to `Ended` (`tddy-changeset/src/session_activity.rs:84-93`), so each of these clears it.

#### `packages/tddy-tools/src/session_hook.rs` (#755)

- `session_status_request(args, stdin_json)` sends the hook's `message` only with `WaitingForInput`.
- Still fail-quiet.

#### `packages/tddy-coder/src/session_participant/pending_question.rs` (#755)

- A tool session's presenter mode becomes a pending question:
  - `Select` → `select`;
  - `MultiSelect` → `multi_select`;
  - `TextInput` → `text`.

#### `packages/tddy-tool-engine/src/session_follow.rs` (#755)

- `FollowPage<T> { items, next_cursor, has_more, clamped_to }`.
- `FOLLOW_PAGE_DEFAULT = 50`, `FOLLOW_PAGE_MAX = 200`.
- A cursor is a decimal position.

#### `packages/tddy-discovery/src/subagent/permission.rs` (`origin/master`, `#coagents`)

- `PermissionRequestView { request_id, tool_call: PermissionToolCall { title, kind, raw_input }, options: Vec<PermissionChoice { option_id, name, kind: allow_once|allow_always|reject_once|reject_always }> }`.
- `PermissionAnswer::{Selected { request_id, option_id }, Cancelled { request_id }}`.
- `AnsweredBy::{MainAgent, User(login)}`.
- `trait PermissionAnswering { answer_as, answered_by, deferred_to_user, pending_request }`: the first
  answer wins.

#### `packages/tddy-service/proto/session_agents.proto` (`origin/master`)

- `AnswerAgentPermissionRequest { session_token, session_id, daemon_instance_id, conversation_id, request_id, option_id, cancelled }` → `{ answered_by }`.

#### `packages/tddy-tool-engine/src/project_home_tools.rs` (`origin/master`, #750)

- `TARGET_DAEMON_INSTANCE_ID = "daemon_instance_id"` and `target_daemon_instance_id(args_json)`.
- The schema says: "The host the session runs on, as ListProjectSessions reports it…; empty for this host".
- "the daemon forwards such a call to that host whole, and it runs there unchanged".

### Findings

- #755 and this stack's old `3/8` recorded the same fact in two stores. #755 landed first, so `3/8`
  extends it.
- `tddy-daemon-kernel` does not depend on `tddy-changeset`. The bus therefore carries a kernel mirror
  (`QuestionNotice`), built in `tddy-session-activity`.
