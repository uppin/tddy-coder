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

The launcher's plans live beside it: `coding_agent_launcher/headless.rs` (the headless plan from the
agent's spec, its conversation files and session pointer, its approvals, and
`sweep_headless_files_left_by_this_daemon`, re-exported from `coding_agent_launcher`) and
`coding_agent_launcher/acp.rs` (the ACP plan from the spec: command, MCP servers, session meta,
route and confinement).

A coding agent can write, so a launch the daemon cannot honour **exactly** is refused with the reason
— never started some weaker way.

## The harness command

A launcher holding the daemon's coding agents (`with_coding_agents`, which `for_daemon` installs
through `host_coding_agents`) plans an ACP launch from the agent's spec (`acp_plan_from_spec`,
`#agent-tools 11/12`): the command renders from `coding_agent.acp.command`, the MCP servers from the
`acp-session` slot (`acp_mcp_servers_from_spec`, sent as `session/new` `mcp_servers`) and a managed
session's withdrawal from `tool_deny` (`acp_session_meta_from_spec`, the `session_meta_for`
override; `config_env` merged into `OPENCODE_CONFIG_CONTENT`). The specs give:

| Harness | Program and args |
|---|---|
| `claude-acp` | `bunx claude-agent-acp`, or the binary `TDDY_CLAUDE_AGENT_ACP_CLI` names (no args; `package_runner` param) |
| `codex-acp` | `codex-acp` beside the resolved `codex` dep (`sibling_of`), or `TDDY_CODEX_ACP_CLI` — never `PATH` |
| `opencode` | `opencode acp` |

The working directory is the session's worktree. Any harness the host's registry does not declare an
ACP agent is refused; nothing is planned by hand, and no launcher names an agent
(`CodingAgentLauncher::session_meta_for` has no default). The OpenCode route config stays inline in
`OPENCODE_CONFIG_CONTENT`, since a route file the daemon writes cannot be read by an agent running as
the session's OS user ([todo](../../../docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md)).

## The provider route

When the definition names a provider account, `route_from_vault` resolves it in the session owner's
vault with `tddy_agent_launch::session_provider_route::resolve_session_route` — the call tool sessions
use — and `apply_route` applies it: `env` and `secret_env` for every harness, `codex_acp_route_args`
(`-c` overrides) for an agent whose spec's `route.shape` is `toml_args` (`codex-acp`), and the route's
config file merged into inline `OPENCODE_CONFIG_CONTENT` for one whose shape is `json_config_file`
(`opencode`) — keyed on the spec, never the harness name (a non-JSON config, the route's or an inline one already
set, is refused). A route that does not resolve refuses the launch.

## A managed session

`refuse_unless_spec_confinable` refuses an agent whose spec cannot withdraw its native tools
(`confinement_of_spec`), naming the registry's confinable ACP agents (`confinable_acp_agents`). A
`config_env` withdrawal (`opencode`) is merged into `OPENCODE_CONFIG_CONTENT`; an `acp_meta` one
(`claude-acp`) travels as `session/new` `_meta` (`session_meta_for`, set by `tddy-session-agents`). `mcp_servers_for`
returns `tddy-tools --mcp` with the environment `tddy_session_split::coding_agent_wiring`'s
`CodingAgentToolHost::tool_server` builds: the colocated jail's `TDDY_REMOTE_*` variables, with
`TDDY_REMOTE_DAEMON_SOCKET` naming the session OS user's agent-tool socket
(`DaemonLauncherDeps.agent_tool_sockets`), and a session token minted for the agent. A user the daemon
cannot give an agent-tool socket — under `tddy-supervisor`, one not listed under `agent_tool_sockets:`
— is refused, naming the setting. A daemon that signs no tokens refuses. An unmanaged launch has no MCP
servers.

## Becoming the session's user

`launch_agent` is the production path: `launch_transport` plus the hooks of the adapter the agent's
spec declares (`coding_agent.acp.hooks_adapter`), prepared on the command before it is spawned or
relayed — `codex-acp-login` hands back its one `codex login` retry on an auth-like `session/new`
error, an agent declaring none gets `no_launch_hooks()`, and a name this build has no adapter for is
refused. The adapter is handed **no route**: the plan already rendered it (`-c` overrides when the
spec's `route.shape` is `toml_args`, the inline `OPENCODE_CONFIG_CONTENT` when it is
`json_config_file`), so `codex-acp-login` never appends a second override and `opencode-route-file`
writes no route file a session user could not read
([`docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md`](../../../docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md)).
The login runs as the daemon's user and writes its authorize URL into the worktree
([`docs/dev/todo/2026-11-09-daemon-codex-acp-login-runs-as-the-daemon-user-in-the-worktree.md`](../../../docs/dev/todo/2026-11-09-daemon-codex-acp-login-runs-as-the-daemon-user-in-the-worktree.md)).
`command_for` covers only the case where the driver spawns the agent itself.

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

## A headless conversation

`headless_command_for` (`headless_plan`) builds the `HeadlessCommand` for `claude`, `cursor`, `codex`
or `oz` from the agent's declaration (`claude.yaml`, `cursor.yaml`, `codex.yaml`, `oz.yaml`), refusing
rather than starting anything weaker:

- the launcher must be wired (`with_headless_wiring(config, <tddy_data_dir>/headless)`, which
  `for_daemon` does) and hold the host's coding agents (`with_coding_agents`; a tool file that does
  not load refuses every headless launch, and nothing is planned by hand), the harness confinable for
  the session (`refuse_unless_registry_confinable` — no headless spec is, since a headless flag for its own tools
  is not an ACP withdrawal, so a managed session refuses), and the
  launch must carry a plain conversation id;
- `HostCodingAgents::resolve` names the agent's spec and binary; the command runs on the launch's
  worktree with the session's model, its argv rendered at each turn by a `SpecHeadlessRenderer` (the
  command's `renderer`) — the command names no program of its own;
- the CLI runs as the daemon's own user: `refuse_unless_daemon_user` probes `run_as_session_user`, and
  any wrap (`setpriv`, a relay) is a refusal;
- a provider route contributes its `env` and `secret_env`; a route that writes a file or passes
  arguments other than `--model <the session's model>` is refused;
- every CLI is handed the `tddy-tools --mcp` spec (`tool_server_spec`, with `TDDY_CONVERSATION_ID`) as
  the command's `mcp_server`, which the spec's MCP slot renders into the agent's own shape at the
  first turn (see [Session agent roster § Coding agents](../../../docs/ft/daemon/session-agent-roster.md#coding-agents)):
  - an agent whose spec sets `headless.approval_tool` (`claude`) gets the prompt tool
    `mcp__tddy-tools__approval_prompt`, and `bind_approvals` registers (conversation, session, the
    spec's `TDDY_REMOTE_SESSION_TOKEN`) in `ApprovalRoutes` (`with_approval_routes`) — **last**, once
    the plan is complete, so a plan that fails part-way leaves no route; a spec with no token is
    refused;
  - an agent whose MCP slot is `inline` (`codex`) registers the server on its argv, where no
    credential may be: `write_conversation_pointer` moves the server's `env` (the conversation id and
    the minted `TDDY_REMOTE_SESSION_TOKEN`) into the conversation's own session pointer,
    `<conversation dir>/.tddy/session.json` — owner-only, written atomically, no socket, the
    variables in its `remote` map, owned by this daemon (`SessionPointerOwner::this_process()`) and
    removed with the directory — and sets the server's `cwd` to the conversation directory, which the
    inline slot renders as `-c mcp_servers.tddy-tools.cwd="<dir>"`. The server's `env` is then empty,
    so the slot renders no `env_vars` and the command's `env` carries no `TDDY_*` variable; the
    `tddy-tools` Codex starts in that directory finds its session through the pointer
    ([tddy-tools session resolution](../../tddy-tools/docs/session-resolution.md)). A pointer that
    cannot be written refuses the launch, naming its path;
  - a slot that writes into the worktree (`cursor`'s `.cursor/mcp.json`) is recorded first
    (`record_written_files`): an existing file there is refused by name, the path is added to the
    repository's `info/exclude` (`tddy_agent_backend::turn_files::exclude_from_git`) before anything
    is written, and a worktree that is not a git checkout is refused;
- the conversation directory is the command's `scratch_dir`, where the turns write their
  registration files; the command's `turn_deadline` is the driver's default.
- the conversation directory is created by `claim_conversation_files`, tagged with this
  daemon's **durable** instance id (`local_base_instance_id_for_config`, held in `HeadlessWiring`),
  and removed again by its `ClaimedFiles` guard unless the plan completes.

`tddy-session-agents`' `AgentRoster::conversation_closed` removes the route and the directory, on a
close and on an open that fails after the plan. `sweep_headless_files_left_by_this_daemon(config,
tddy_data_dir)` removes the directories this daemon's previous run tagged; the daemon runtime's
`build_session_host` calls it before the host serves, logging a failure rather than refusing to
start. Cursor's worktree file is not removed on close, and the start-up call has no automated test —
see the backlog.

Approvals are bound for `claude` only: `cursor`, `codex` and `oz` have no permission-prompt tool.
`refuse_unless_registry_confinable` still refuses all three in a managed session
([`docs/dev/todo/2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md`](../../../docs/dev/todo/2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md)).
