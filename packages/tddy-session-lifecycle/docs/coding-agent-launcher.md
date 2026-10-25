# Coding-agent launcher

`DaemonCodingAgentLauncher` (`src/coding_agent_launcher.rs`) is the daemon's implementation of
`tddy_coding_subagent::CodingAgentLauncher`: it turns a `CodingAgentLaunch` into a running ACP agent
for one coding-agent conversation. Product behaviour:
[Session agent roster § Coding agents](../../../docs/ft/daemon/session-agent-roster.md#coding-agents).
The runtime installs it with `DaemonCodingAgentLauncher::for_daemon(DaemonLauncherDeps { config,
tddy_data_dir, spawn_backend, vaults, user_resolver, tokens })` — `spawn_backend` is the
`SpawnBackendChoice` the daemon read from its environment at startup, and a `Supervisor` choice is
what gives the launcher its relay — and hands it to the host with
`DaemonSessionHost::with_coding_agent_launcher`; `tddy-session-agents` reaches it through
`AgentRoster.coding_agent_launcher`.

A coding agent can write, so a launch the daemon cannot honour **exactly** is refused with the reason
— never started some weaker way.

## The harness command

| Harness | Program and args |
|---|---|
| `claude-acp` | `bunx claude-agent-acp`, or the binary `TDDY_CLAUDE_AGENT_ACP_CLI` names (no args) |
| `codex-acp` | `codex-acp` on `PATH`, or `TDDY_CODEX_ACP_CLI` |
| `opencode` | `opencode acp` |

The working directory is the session's worktree. Any other harness is refused.

## The provider route

When the definition names a provider account, `route_from_vault` resolves it in the session owner's
vault with `tddy_agent_launch::session_provider_route::resolve_session_route` — the call tool sessions
use — and `apply_route` applies it: `env` and `secret_env` for every harness, `codex_acp_route_args`
(`-c` overrides) for `codex-acp`, and the route's config file merged into inline
`OPENCODE_CONFIG_CONTENT` for `opencode` (a non-JSON config, the route's or an inline one already
set, is refused). A route that does not resolve refuses the launch.

## A managed session

`refuse_unless_confinable` refuses a harness that cannot withdraw its native tools. For `opencode`
the withdrawal config (`opencode_withdrawal_config`) is merged into `OPENCODE_CONFIG_CONTENT`; for
`claude-acp` it travels as `session/new` `_meta` (set by `tddy-session-agents`). `mcp_servers_for`
returns `tddy-tools --mcp` with the environment `tddy_session_split::coding_agent_wiring`'s
`CodingAgentToolHost::tool_server` builds: the colocated jail's `TDDY_REMOTE_*` variables and a session
token minted for the agent. A daemon that signs no tokens refuses. An unmanaged launch has no MCP
servers.

## Becoming the session's user

`launch_transport` is the production path; `command_for` covers only the case where the driver
spawns the agent itself.

1. **The daemon's own user** — the command as built (`AgentTransport::Command`).
2. **A privileged daemon** — `coding_agent_as_user::run_as_session_user` wraps the argv in `setpriv`
   with the user's `HOME`/`PATH` (the PTY path's privilege drop), dropping the GitHub token variables
   the daemon may have exported. The child otherwise inherits the daemon's environment, as a PTY
   session does.
3. **An unprivileged daemon under `tddy-supervisor`** — `coding_agent_relay::start_relayed` binds a
   per-launch socket in `<tddy_data_dir>/relay/` (the directory is held owner-only: an existing one is
   narrowed to `0700`, a symlink or a foreign owner is refused), sends `SpawnRelayed` with the
   harness resolved to an absolute path, waits up to 15 s for the supervisor to connect, and returns
   the stream (`AgentTransport::Stream`), which the ACP driver serves with `AcpDriver::spawn_over`.
   The socket file is removed once connected. The supervisor's opaque denial becomes an error naming
   the three settings an operator can change: `allowed_relay_tool_paths`, `allowed_session_users`,
   `allowed_env_keys`. Closing the stream stops the agent.
4. **Anything else** — refused, naming the user.

On a supervised host the program sent is what `allowed_relay_tool_paths` must list. Its arguments are
not allowlisted, so allowlisting `bunx` lets the daemon run any npm package as the session user; set
`TDDY_CLAUDE_AGENT_ACP_CLI` and allowlist that binary instead.
