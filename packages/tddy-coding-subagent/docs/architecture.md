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
| `session` | `CodingAgentSession` — one `AcpDriver` (one agent process) per conversation |

## The launcher port

```rust
trait CodingAgentLauncher {
    async fn command_for(&self, launch) -> Result<AcpAgentCommand, SubagentError>;   // driver spawns it
    async fn launch_transport(&self, launch) -> Result<AgentTransport, SubagentError>; // production path
    async fn mcp_servers_for(&self, launch) -> Result<Vec<McpServerSpec>, SubagentError>;
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

`cancel_handle()` returns a `CancelHandle` that sends `session/cancel` (the turn ends `cancelled`).
The daemon's conversation table holds `Box<dyn SubagentSession>` and does not reach it yet; its
`subagent_cancel` closes the conversation, which drops the session and stops the process.
