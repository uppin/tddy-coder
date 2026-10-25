# tddy-coding-subagent — architecture

A **coding agent** (Claude Code over `claude-agent-acp`, Codex over `codex-acp`, OpenCode over
`opencode acp`) as a session subagent: a `tddy_discovery::subagent::SubagentSession` the daemon's
conversation table drives exactly like a model definition's loop. Product behaviour:
[Session agent roster § Coding agents](../../../docs/ft/daemon/session-agent-roster.md#coding-agents).

The crate depends on `tddy-discovery` only for the `SubagentSession` trait and outcome types, and on
`tddy-agent-backend` for the ACP driver. It knows nothing about the daemon: who the session's user
is, which route applies and how a process becomes that user are the **launcher's** answers, which
the daemon implements (`tddy-session-lifecycle::coding_agent_launcher`).

## Modules

| Module | Role |
|---|---|
| `catalog` | `CodingAgentDef { name, label, harness, model, provider_account }` from ACP harness profiles (`coding_agent_defs_in`) and ACP `allowed_agents` ids, profiles first; a profile shadows an allowed id of the same name. `ACP_HARNESSES = [CLAUDE_ACP, CODEX_ACP, OPENCODE]` |
| `confinement` | `confinement_for(harness)` — `WithdrawsNativeTools` or `CannotWithdraw`; `refuse_unless_confinable(harness, managed)` is the one refusal both attach and launch use; `withdrawal_session_meta` (claude) and `opencode_withdrawal_config` (opencode) are the withdrawal itself |
| `launcher` | `CodingAgentLauncher` port and `CodingAgentLaunch` (def, session id/user/token, worktree, managed) |
| `permissions` | `DenyAndRecord` — the ACP `PermissionHandler` that rejects every request and records it for the turn's messages |
| `session` | `CodingAgentSession` — one `dyn ConversationDriver` per conversation: an `AcpConversation` (one ACP agent process) or a `HeadlessDriver` (one CLI invocation per turn) |
| `transports` | `HEADLESS_HARNESSES = ["claude", "cursor", "codex", "oz"]`; `transport_of(harness)` (`Acp` / `Headless` / `None` for PTY-only), `permission_policy_of(harness)` (`AsksCaller` / `Fixed(text)`), `transport_and_policy_wire` — the catalog row's `transport` / `permission_policy` spellings, derived from the harness id so `CodingAgentDef` is unchanged |
| `approval_routes` | `ApprovalRoutes` — the daemon's map of headless conversations to the launch that owns them and the `ParkedPermissions` their approvals park on |
| `parked_permissions` | `ParkedPermissions` — the agent's permission requests, parked oldest first until answered; `answer` (the main agent's), `answer_recording(answer, by)` (first answer wins, a later one refused naming who answered), `defer_to_user`, `cancel_request`, `cancel_pending` |
| `user_permission_answers` | `PermissionAnswering` for `ParkedPermissions` — the handle the daemon answers a deferred request through. The trait and `AnsweredBy { MainAgent, User(login) }` live in `tddy_discovery::subagent` and are re-exported here |
| `permission_records` (private) | the `AcpUpdate::ToolCallRecord`s a request leaves on the update stream: pending while deferred to the user, completed with `{answeredBy, optionId}` or `{answeredBy, cancelled: true}` once answered |

## The launcher port

```rust
trait CodingAgentLauncher {
    async fn command_for(&self, launch) -> Result<AcpAgentCommand, SubagentError>;   // driver spawns it
    async fn launch_transport(&self, launch) -> Result<AgentTransport, SubagentError>; // production path
    async fn mcp_servers_for(&self, launch) -> Result<Vec<McpServerSpec>, SubagentError>;
    async fn headless_command_for(&self, launch) -> Result<HeadlessCommand, SubagentError>; // default: refused
}
enum AgentTransport { Command(AcpAgentCommand), Stream(UnixStream) }
```

`launch_transport` returns either a command the ACP driver spawns itself (the daemon's own user,
or a privileged daemon behind `setpriv`) or an already-connected stream (a relayed spawn through
`tddy-supervisor`), which the driver serves with `AcpDriver::spawn_over`. `mcp_servers_for` is asked
**before** the transport, so a refusal never starts a process; a managed launch gets
`[tddy-tools --mcp]` (`TDDY_TOOLS_SERVER`, `TDDY_TOOLS_MCP_ARG`), an unmanaged one none.

## Confinement per harness

| Harness | Mechanism | Managed sessions |
|---|---|---|
| `claude-acp` | `session/new` `_meta.claudeCode.options.disallowedTools` = Read, Write, Edit, MultiEdit, NotebookEdit, Bash, BashOutput, KillShell, Glob, Grep, LS | allowed |
| `opencode` | inline `OPENCODE_CONFIG_CONTENT` `{"tools": {bash, read, write, edit, patch, glob, grep, list: false}}`, merged over the route's config | allowed |
| `codex-acp` | sandbox modes only, no tool withdrawal | refused at attach and at launch |

Both withdrawal mechanisms follow each adapter's documentation. That the `_meta` reaches
`session/new` is tested against `tddy-acp-stub`; neither has been verified against a live
`claude-agent-acp` or OpenCode.

## The session

`CodingAgentSession::start(transport, open, model)` spawns or serves the agent, opens one ACP session
(`OpenSession { cwd, mcp_servers, meta }`) and selects the model. Per turn:

- `take_turn` sends one `session/prompt` and maps the ACP stop reason to `StopReason`;
- `describe_turn` turns the turn's ACP tool calls into one `MessageDescriptor` each (ids from
  `MessageId::from_ordinal`, previews from `tddy_discovery::subagent::preview_of`), plus one
  `is_error` descriptor per permission `DenyAndRecord` refused;
- loop-only request options (`maxTurns`, `yieldConditions`, `fromMessageId`, `correction`,
  `replacement`) are refused with `SubagentError::unsupported_by_coding_agent(option)`;
- dropping the session stops the agent process.

A turn that answers a pending request as the main agent delivers the answer **before** it takes the
in-flight slot. When a turn already holds the slot — a request deferred to the user keeps its turn
open until it is answered — the answer is delivered anyway and the call is refused at once
(`answer_delivered_while_held`), since waiting behind the turn it releases would never return. A
deferral (`TurnRequest::deferring_permission_to_user`) is accepted only for the pending request it
names: the turn records the request as deferred, streams its pending record, and waits in `settle`
for either side's answer, recording that answer when it is given. `permission_answerer()` exposes the
session's `ParkedPermissions` as `dyn PermissionAnswering`, so the daemon answers without the
conversation's lock.

`cancel_handle()` returns a `CancelHandle` that sends `session/cancel` (the turn ends `cancelled`).
The daemon's conversation table holds `Box<dyn SubagentSession>` and does not reach it yet; its
`subagent_cancel` closes the conversation, which drops the session and stops the process.

## Headless sessions

`CodingAgentSession::start_headless(harness, command, model)` holds a `HeadlessDriver`; it is asked
for when `transport_of(def.harness)` is `Headless` (`tddy-session-agents` calls the launcher's
`headless_command_for`). Turns, outcome mapping and transcript recording are the ACP session's.
`permissions()` returns the session's `ParkedPermissions`, which the opener registers in
`ApprovalRoutes` (`register_permissions`) so a headless claude's approval parks where the yield
machinery of `#coagents` 3/8 reads it.

## Approval routes

`ApprovalRoutes` (shared `Arc` between the launcher, the conversation open/close paths and the
exec-tool handler) keys a route by conversation id:

- `register_launch(conversation, session_id, session_token)` — at launch, the session and the
  credential minted for the agent's `tddy-tools` (`SESSION_TOKEN_ENV`). The minted token carries no
  conversation claim, so the binding is made here.
- `register_permissions(conversation, permissions)` — once the session is open.
- `request(conversation, session_id, session_token, tool, input)` — refused (`Err`) unless the route
  has both halves and both credentials match (constant-time); otherwise parks an `allow` / `reject`
  request and waits up to 60 s (`with_timeout`). A timeout, a cancel or a dropped caller (a `Parking`
  guard, `ParkedPermissions::cancel_request`) is `Ok(false)`.
- `remove(conversation)` — on close; cancels anything still parked.
