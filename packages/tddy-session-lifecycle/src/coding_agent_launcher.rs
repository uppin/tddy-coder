//! The daemon's [`CodingAgentLauncher`]: turns a coding-agent launch into the ACP agent command.
//!
//! A coding agent is a subagent that can write, so a launch the daemon cannot honour *exactly* is
//! refused with a typed reason rather than started some weaker way:
//!
//! - **The session's OS user** — [`crate::coding_agent_as_user`]: run behind `setpriv` when this
//!   daemon is privileged; on a supervised host started by `tddy-supervisor`'s `SpawnRelayed`,
//!   which relays the child's stdio over a socket ([`crate::coding_agent_relay`]); refused when
//!   neither is available.
//! - **A provider account's route** — resolved from the session owner's vault through
//!   [`tddy_agent_launch::session_provider_route::resolve_session_route`], exactly as a tool session
//!   resolves it, and applied per harness: environment and secret environment for all of them,
//!   `-c` overrides for `codex-acp`, the route's config file as inline `OPENCODE_CONFIG_CONTENT` for
//!   `opencode`.
//! - **A managed session** — the harness's native tools are withdrawn
//!   ([`tddy_coding_subagent::opencode_withdrawal_config`] for `opencode`; `claude-acp` through
//!   `_meta` at `session/new`) and
//!   the agent is handed `tddy-tools --mcp` with a credential minted for it
//!   ([`tddy_session_split::coding_agent_wiring`]).
//!
//! **A headless harness** (`claude`, `cursor`, `codex`, `oz`) runs its CLI once per turn rather than
//! speaking ACP: [`DaemonCodingAgentLauncher::headless_command_for`] names that CLI, the binary a
//! daemon's own session of it resolves, on the conversation's worktree, with the route's environment.
//! It runs only as the daemon's own user, so a session of another OS user is refused. A `claude`
//! conversation is also handed `tddy-tools --mcp` through an owner-only MCP config under the daemon's
//! data directory, its permission prompts routed to that server. Cursor, Codex and Oz take no MCP
//! config yet.
//!
//! **The harness binary.** `claude-acp` runs as `bunx claude-agent-acp` unless
//! `TDDY_CLAUDE_AGENT_ACP_CLI` names a `claude-agent-acp` binary; `codex-acp` as `codex-acp` unless
//! `TDDY_CODEX_ACP_CLI` names one. On a supervised host the program is what
//! `allowed_relay_tool_paths` must list, and its arguments are not allowlisted — so allowlisting
//! `bunx` lets the session user run any npm package; set `TDDY_CLAUDE_AGENT_ACP_CLI` and allowlist
//! that binary instead.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_coding_subagent::launcher::{HEADLESS_DIR, SESSION_TOKEN_ENV};
use tddy_coding_subagent::{
    opencode_withdrawal_config, refuse_unless_confinable, AgentTransport, ApprovalRoutes,
    CodingAgentLaunch, CodingAgentLauncher, CLAUDE_ACP, CODEX_ACP, CONVERSATION_ID_ENV, OPENCODE,
    OPENCODE_CONFIG_CONTENT, TDDY_TOOLS_MCP_ARG, TDDY_TOOLS_SERVER,
};
use tddy_core::provider_route::{Harness, HarnessRoute, RouteChoice, RouteSources};
use tddy_core::{AcpAgentCommand, HeadlessCommand, HeadlessHarness, McpServerSpec};
use tddy_daemon_kernel::config::{
    resolve_claude_binary_path, resolve_cursor_binary_path, DaemonConfig,
};
use tddy_discovery::subagent::SubagentError;
use tddy_subagent_worktree::ConversationId;

use crate::coding_agent_as_user::{run_as_session_user, AsUserRefusal, DropContext};
use crate::coding_agent_relay::{start_relayed, RelayEndpoint};

/// Env var naming the `codex-acp` binary, as the workflow backend reads it.
const CODEX_ACP_CLI_ENV: &str = "TDDY_CODEX_ACP_CLI";
/// The `codex-acp` binary looked up on `PATH` when [`CODEX_ACP_CLI_ENV`] is unset.
const CODEX_ACP_BINARY: &str = "codex-acp";
/// Env var naming a `claude-agent-acp` binary to run in place of `bunx claude-agent-acp` — what a
/// supervised host allowlists instead of `bunx`, whose arguments the allowlist cannot pin.
const CLAUDE_AGENT_ACP_CLI_ENV: &str = "TDDY_CLAUDE_AGENT_ACP_CLI";
/// The package runner `claude-agent-acp` runs through when [`CLAUDE_AGENT_ACP_CLI_ENV`] is unset.
const BUNX: &str = "bunx";
/// The npm package (and binary) of Claude Code's ACP adapter.
const CLAUDE_AGENT_ACP: &str = "claude-agent-acp";
/// The `opencode` binary.
const OPENCODE_BINARY: &str = "opencode";
/// The `opencode` subcommand that speaks ACP over stdio.
const OPENCODE_ACP_SUBCOMMAND: &str = "acp";
/// The directory under `<tddyhome>` a supervised launch's relay sockets are bound in.
const RELAY_SOCKET_DIR: &str = "relay";
/// The MCP config file a headless CLI reads, in its conversation's directory.
const MCP_CONFIG_FILE: &str = "mcp.json";
/// The mode the MCP config is written with. It carries the agent's tool credential, so only its
/// owner may read it.
const MCP_CONFIG_MODE: u32 = 0o600;
/// The tool Claude Code's permission prompts are routed to: `approval_prompt` on the `tddy-tools`
/// server (`mcp__<server>__<tool>`).
const APPROVAL_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";
/// The `oz` binary a headless run is started with, looked up on `PATH`.
const OZ_BINARY: &str = "oz";

/// Resolves the provider route a launch runs on: `None` for the harness's own login.
pub type RouteResolver =
    Arc<dyn Fn(&CodingAgentLaunch) -> Result<Option<HarnessRoute>, SubagentError> + Send + Sync>;

/// The `tddy-tools` binary and environment a managed launch's MCP server runs with.
pub type ToolServerResolver = Arc<
    dyn Fn(&CodingAgentLaunch) -> Result<(PathBuf, Vec<(String, String)>), SubagentError>
        + Send
        + Sync,
>;

/// What of a running daemon [`DaemonCodingAgentLauncher::for_daemon`] builds its launcher from.
pub struct DaemonLauncherDeps {
    pub config: tddy_daemon_kernel::config::DaemonConfig,
    pub tddy_data_dir: PathBuf,
    /// The spawn backend the daemon read from its environment at startup: a supervised daemon
    /// relays a coding agent that must run as another user through that supervisor.
    pub spawn_backend: tddy_spawn::supervisor_client::SpawnBackendChoice,
    /// The session owners' vaults a provider account's route is read from.
    pub vaults: Option<Arc<tddy_daemon_auth::SessionVaults>>,
    pub user_resolver: tddy_daemon_kernel::SessionUserResolver,
    /// What the agent's own tool credential is minted with; `None` for a daemon that signs none.
    pub tokens: Option<tddy_daemon_auth::SessionTokens>,
}

/// What a headless conversation is launched with: the daemon's config, which names the CLI binaries
/// the way the daemon's own sessions of those CLIs do, and the directory a conversation keeps the MCP
/// config its CLI reads.
#[derive(Clone)]
struct HeadlessWiring {
    config: Arc<DaemonConfig>,
    data_dir: PathBuf,
}

/// Starts the harness a launch names, as the session's OS user, on the session's worktree.
#[derive(Clone)]
pub struct DaemonCodingAgentLauncher {
    drop: DropContext,
    route: RouteResolver,
    tool_server: ToolServerResolver,
    /// How an unprivileged daemon starts an agent as another user: through `tddy-supervisor`.
    relay: Option<RelayEndpoint>,
    /// What a headless conversation is launched with; `None` on a launcher not wired for one, which
    /// refuses a headless launch naming that.
    headless: Option<HeadlessWiring>,
    /// Where a headless conversation's permission approvals are bound at launch. `None` leaves them
    /// unbound, so every approval for such a conversation is refused.
    approval_routes: Option<Arc<ApprovalRoutes>>,
}

impl DaemonCodingAgentLauncher {
    /// A launcher for this process's identity.
    #[must_use]
    pub fn new(route: RouteResolver, tool_server: ToolServerResolver) -> Self {
        Self::with_identity(DropContext::current(), route, tool_server)
    }

    /// With the supervisor to ask when this daemon cannot become the session's user itself.
    #[must_use]
    pub fn with_supervisor_relay(mut self, relay: RelayEndpoint) -> Self {
        self.relay = Some(relay);
        self
    }

    /// With the daemon's config and data directory, a headless coding agent can be opened through
    /// this launcher: its CLI resolves as the daemon's own sessions of that CLI do, and a claude
    /// conversation keeps its MCP config under `data_dir`.
    #[must_use]
    pub fn with_headless_wiring(mut self, config: DaemonConfig, data_dir: PathBuf) -> Self {
        self.headless = Some(HeadlessWiring {
            config: Arc::new(config),
            data_dir,
        });
        self
    }

    /// Bind each headless conversation's permission approvals through `routes` (builder): the launch
    /// registers the session it runs for and the credential its agent is minted with.
    #[must_use]
    pub fn with_approval_routes(mut self, routes: Arc<ApprovalRoutes>) -> Self {
        self.approval_routes = Some(routes);
        self
    }

    /// The launcher a running daemon installs: routes from the session owner's vault, tool servers
    /// from this daemon's own tokens and toolchain. `spawn_backend` is the choice the daemon read
    /// from its environment at startup: a supervised daemon relays through that supervisor.
    #[must_use]
    pub fn for_daemon(deps: DaemonLauncherDeps) -> Self {
        let DaemonLauncherDeps {
            config,
            tddy_data_dir,
            spawn_backend,
            vaults,
            user_resolver,
            tokens,
        } = deps;
        let relay_dir = tddy_data_dir.join(RELAY_SOCKET_DIR);
        let headless_dir = tddy_data_dir.join(HEADLESS_DIR);
        let headless_config = config.clone();
        let supervisor_socket = match spawn_backend {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                Some(socket_path)
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => None,
        };
        let route = route_from_vault(move |token| {
            tddy_agent_launch::session_acting_identity::SessionAccountAccess::new(
                vaults.clone(),
                Arc::clone(&user_resolver),
                token,
            )
        });
        let tool_host = tddy_session_split::coding_agent_wiring::CodingAgentToolHost {
            config,
            tddy_data_dir,
            tokens,
        };
        let tool_server: ToolServerResolver = Arc::new(move |launch| {
            tool_host
                .tool_server(&launch.session_id, &launch.session_token)
                .map_err(|status| SubagentError::from(status.message().to_string()))
        });
        let launcher =
            Self::new(route, tool_server).with_headless_wiring(headless_config, headless_dir);
        match supervisor_socket {
            Some(socket_path) => launcher.with_supervisor_relay(RelayEndpoint {
                spawner: Arc::new(crate::coding_agent_relay::SupervisorRelay { socket_path }),
                socket_dir: relay_dir,
            }),
            None => launcher,
        }
    }

    /// A launcher whose identity is `drop` (tests).
    #[must_use]
    pub fn with_identity(
        drop: DropContext,
        route: RouteResolver,
        tool_server: ToolServerResolver,
    ) -> Self {
        Self {
            drop,
            route,
            tool_server,
            relay: None,
            headless: None,
            approval_routes: None,
        }
    }

    fn refuse(launch: &CodingAgentLaunch, reason: &str) -> SubagentError {
        SubagentError::from(format!(
            "coding agent '{}' ({}) cannot be launched: {reason}",
            launch.def.name, launch.def.harness
        ))
    }

    fn harness_command(
        launch: &CodingAgentLaunch,
    ) -> Result<(PathBuf, Vec<String>), SubagentError> {
        match launch.def.harness.as_str() {
            CLAUDE_ACP => Ok(claude_agent_acp_command(std::env::var_os(
                CLAUDE_AGENT_ACP_CLI_ENV,
            ))),
            CODEX_ACP => Ok((
                std::env::var_os(CODEX_ACP_CLI_ENV)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(CODEX_ACP_BINARY)),
                Vec::new(),
            )),
            OPENCODE => Ok((
                PathBuf::from(OPENCODE_BINARY),
                vec![OPENCODE_ACP_SUBCOMMAND.to_string()],
            )),
            other => Err(Self::refuse(
                launch,
                &format!("this daemon has no ACP command for the '{other}' harness"),
            )),
        }
    }

    /// Apply `route` to the harness command: every harness gets its environment; `codex-acp` its
    /// `-c` overrides; `opencode` its config file as inline config.
    fn apply_route(
        launch: &CodingAgentLaunch,
        command: &mut AcpAgentCommand,
        route: &HarnessRoute,
    ) -> Result<(), SubagentError> {
        command.env.extend(route.env.iter().cloned());
        command.env.extend(
            route
                .secret_env
                .iter()
                .map(|(name, secret)| (name.clone(), secret.expose().to_string())),
        );
        match launch.def.harness.as_str() {
            CODEX_ACP => command.args.extend(tddy_core::codex_acp_route_args(route)),
            OPENCODE => {
                if let Some(config) = Self::opencode_route_config(launch, route)? {
                    set_opencode_config(command, config)
                        .map_err(|reason| Self::refuse(launch, &reason))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The route's OpenCode config file as JSON; `None` for a route that carries none.
    fn opencode_route_config(
        launch: &CodingAgentLaunch,
        route: &HarnessRoute,
    ) -> Result<Option<serde_json::Value>, SubagentError> {
        let Some(file) = route.files.first() else {
            return Ok(None);
        };
        serde_json::from_str(&file.contents).map(Some).map_err(|e| {
            Self::refuse(
                launch,
                &format!("the route's OpenCode config is not JSON: {e}"),
            )
        })
    }

    /// The command for `launch` as the harness runs it, before it is made to run as another user.
    fn command_before_user_switch(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        refuse_unless_confinable(&launch.def.harness, launch.managed)
            .map_err(|reason| Self::refuse(launch, &reason))?;
        let (program, args) = Self::harness_command(launch)?;
        let mut command = AcpAgentCommand {
            program,
            args,
            env: Vec::new(),
            cwd: Some(launch.worktree.clone()),
        };
        if let Some(account) = launch.def.provider_account.as_deref() {
            let route = (self.route)(launch)?.ok_or_else(|| {
                Self::refuse(
                    launch,
                    &format!("provider account '{account}' resolved to no route"),
                )
            })?;
            Self::apply_route(launch, &mut command, &route)?;
        }
        let withdrawal = launch
            .managed
            .then(|| opencode_withdrawal_config(&launch.def.harness))
            .flatten();
        if let Some(withdrawal) = withdrawal {
            set_opencode_config(&mut command, withdrawal)
                .map_err(|reason| Self::refuse(launch, &reason))?;
        }
        Ok(command)
    }

    fn plan(&self, launch: &CodingAgentLaunch) -> Result<AcpAgentCommand, SubagentError> {
        let command = self.command_before_user_switch(launch)?;
        run_as_session_user(command, &launch.session_os_user, &self.drop)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))
    }

    /// The `tddy-tools` MCP server a managed session's agent is handed, its environment naming the
    /// conversation when it has a worktree of its own.
    fn tool_server_spec(&self, launch: &CodingAgentLaunch) -> Result<McpServerSpec, SubagentError> {
        let (command, mut env) = (self.tool_server)(launch)?;
        // A conversation with a worktree of its own names it, so its exec calls run in that
        // worktree; a launch without one leaves the variable unset and its calls run in the session's.
        if let Some(conversation_id) = &launch.conversation_id {
            env.push((CONVERSATION_ID_ENV.to_string(), conversation_id.clone()));
        }
        Ok(McpServerSpec {
            name: TDDY_TOOLS_SERVER.to_string(),
            command,
            args: vec![TDDY_TOOLS_MCP_ARG.to_string()],
            env,
        })
    }

    /// The command a headless conversation of `launch` runs its CLI with. Refused rather than started
    /// some weaker way: a launch the daemon cannot honour exactly is named, not run.
    fn headless_plan(&self, launch: &CodingAgentLaunch) -> Result<HeadlessCommand, SubagentError> {
        let wiring = self.headless.as_ref().ok_or_else(|| {
            Self::refuse(launch, "this daemon has no headless coding-agent wiring")
        })?;
        refuse_unless_confinable(&launch.def.harness, launch.managed)
            .map_err(|reason| Self::refuse(launch, &reason))?;
        let harness = HeadlessHarness::from_id(&launch.def.harness)
            .ok_or_else(|| Self::refuse(launch, "this daemon has no headless command for it"))?;
        // A conversation's tool calls are named by its id, so a headless one without an id has
        // nothing to name them by, and no id is guessed.
        let conversation = launch.conversation_id.as_deref().ok_or_else(|| {
            Self::refuse(
                launch,
                "a headless conversation needs a conversation id to name its tool calls by, and none was given",
            )
        })?;
        let conversation = ConversationId::parse(conversation).map_err(|unsafe_id| {
            Self::refuse(
                launch,
                &format!("its conversation id {unsafe_id} is not a plain name"),
            )
        })?;
        let program = headless_program(&wiring.config, harness);
        self.refuse_unless_daemon_user(launch, &program)?;
        let env = match launch.def.provider_account.as_deref() {
            Some(account) => {
                let route = (self.route)(launch)?.ok_or_else(|| {
                    Self::refuse(
                        launch,
                        &format!("provider account '{account}' resolved to no route"),
                    )
                })?;
                headless_route_env(launch, &route)?
            }
            None => Vec::new(),
        };
        let (mcp_config, permission_prompt_tool) = match harness {
            HeadlessHarness::Claude => {
                let server = self.tool_server_spec(launch)?;
                self.bind_approvals(launch, conversation.as_str(), &server)?;
                let path = write_mcp_config(&wiring.data_dir.join(conversation.as_str()), &server)
                    .map_err(|e| {
                        Self::refuse(launch, &format!("its MCP config could not be written: {e}"))
                    })?;
                (Some(path), Some(APPROVAL_PROMPT_TOOL.to_string()))
            }
            HeadlessHarness::Cursor | HeadlessHarness::Codex | HeadlessHarness::Oz => (None, None),
        };
        Ok(HeadlessCommand {
            program: PathBuf::from(program),
            env,
            cwd: Some(launch.worktree.clone()),
            model: Some(launch.def.model.clone()),
            mcp_config,
            permission_prompt_tool,
            disallowed_tools: Vec::new(),
            profile: None,
            mcp_server: None,
        })
    }

    /// Binds the permission approvals of conversation `conversation` to the session `launch` runs for
    /// and to the credential its tool server is handed, so only the agent of this launch can ask for
    /// them. Refused, rather than launched unbound, when the tool server carries no credential to bind.
    fn bind_approvals(
        &self,
        launch: &CodingAgentLaunch,
        conversation: &str,
        server: &McpServerSpec,
    ) -> Result<(), SubagentError> {
        let Some(routes) = &self.approval_routes else {
            return Ok(());
        };
        let token = server
            .env
            .iter()
            .find(|(name, _)| name == SESSION_TOKEN_ENV)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| {
                Self::refuse(
                    launch,
                    "its tool server carries no credential to bind its permission approvals to",
                )
            })?;
        routes.register_launch(conversation, &launch.session_id, token);
        Ok(())
    }

    /// Refuses a headless launch whose session runs as another OS user. The headless driver spawns its
    /// CLI as this daemon's own user and cannot wrap it to run as another, so the one check that
    /// [`run_as_session_user`] makes for an ACP agent is made here: a wrap is a refusal.
    fn refuse_unless_daemon_user(
        &self,
        launch: &CodingAgentLaunch,
        program: &str,
    ) -> Result<(), SubagentError> {
        let probe = AcpAgentCommand {
            program: PathBuf::from(program),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        };
        let as_user = run_as_session_user(probe.clone(), &launch.session_os_user, &self.drop)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        if as_user.program != probe.program {
            return Err(Self::refuse(
                launch,
                &format!(
                    "the session runs as '{}', and a headless CLI runs only as the daemon's own user",
                    launch.session_os_user
                ),
            ));
        }
        Ok(())
    }
}

/// The program a headless harness's CLI runs. Claude and Cursor run the binary a daemon's own session
/// of that CLI resolves; Codex and Oz have no such resolver, so they run by name from `PATH`.
fn headless_program(config: &DaemonConfig, harness: HeadlessHarness) -> String {
    match harness {
        HeadlessHarness::Claude => resolve_claude_binary_path(config),
        HeadlessHarness::Cursor => resolve_cursor_binary_path(config),
        HeadlessHarness::Codex => tddy_core::CodexBackend::DEFAULT_CLI_BINARY.to_string(),
        HeadlessHarness::Oz => OZ_BINARY.to_string(),
    }
}

/// The environment a headless CLI takes from `route`: its plain pairs and its secret pairs. A headless
/// CLI reads no route file and takes no route argument but its model, which is the session's own
/// model; the route's environment pins its provider's model. Anything else the route needs is refused,
/// since the headless command has no field to carry it.
fn headless_route_env(
    launch: &CodingAgentLaunch,
    route: &HarnessRoute,
) -> Result<Vec<(String, String)>, SubagentError> {
    if !route.files.is_empty() {
        return Err(DaemonCodingAgentLauncher::refuse(
            launch,
            "its provider route writes a configuration file, which a headless CLI does not read",
        ));
    }
    let names_the_session_model = match route.args.as_slice() {
        [] => true,
        [flag, model] => flag == "--model" && *model == launch.def.model,
        _ => false,
    };
    if !names_the_session_model {
        return Err(DaemonCodingAgentLauncher::refuse(
            launch,
            "its provider route passes arguments a headless CLI cannot take",
        ));
    }
    let mut env = route.env.clone();
    env.extend(
        route
            .secret_env
            .iter()
            .map(|(name, secret)| (name.clone(), secret.expose().to_string())),
    );
    Ok(env)
}

/// Writes the MCP config a headless CLI reads, registering `server` under its name, to `mcp.json` in
/// `dir`, owner only: the config carries the agent's tool credential. Returns the file's path. Its
/// contents are never logged.
fn write_mcp_config(dir: &Path, server: &McpServerSpec) -> io::Result<PathBuf> {
    let env: serde_json::Map<String, serde_json::Value> = server
        .env
        .iter()
        .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
        .collect();
    let mut servers = serde_json::Map::new();
    servers.insert(
        server.name.clone(),
        serde_json::json!({
            "command": server.command.display().to_string(),
            "args": server.args,
            "env": env,
        }),
    );
    let config = serde_json::json!({ "mcpServers": servers });
    let path = dir.join(MCP_CONFIG_FILE);
    tddy_core::atomic_file::write_atomic_with_mode(&path, config.to_string(), MCP_CONFIG_MODE)?;
    Ok(path)
}

/// The program and arguments `claude-acp` runs as: the `claude-agent-acp` binary `override_cli`
/// names, on its own; or, when none is named, `bunx claude-agent-acp`.
fn claude_agent_acp_command(override_cli: Option<OsString>) -> (PathBuf, Vec<String>) {
    match override_cli {
        Some(cli) => (PathBuf::from(cli), Vec::new()),
        None => (PathBuf::from(BUNX), vec![CLAUDE_AGENT_ACP.to_string()]),
    }
}

/// Merge `config` into the command's inline OpenCode config, creating it when absent. An existing
/// inline config that is not JSON is refused naming it, never silently replaced.
fn set_opencode_config(
    command: &mut AcpAgentCommand,
    config: serde_json::Value,
) -> Result<(), String> {
    let existing = match command
        .env
        .iter()
        .position(|(name, _)| name == OPENCODE_CONFIG_CONTENT)
    {
        Some(at) => serde_json::from_str::<serde_json::Value>(&command.env[at].1)
            .map(|parsed| {
                command.env.remove(at);
                Some(parsed)
            })
            .map_err(|e| {
                format!("the {OPENCODE_CONFIG_CONTENT} it already sets is not JSON: {e}")
            })?,
        None => None,
    };
    let mut merged = existing.unwrap_or_else(|| serde_json::json!({}));
    merge_json(&mut merged, config);
    command
        .env
        .push((OPENCODE_CONFIG_CONTENT.to_string(), merged.to_string()));
    Ok(())
}

/// Merge `from` into `into`: objects merge key by key, anything else is replaced.
fn merge_json(into: &mut serde_json::Value, from: serde_json::Value) {
    match (into, from) {
        (serde_json::Value::Object(into), serde_json::Value::Object(from)) => {
            for (key, value) in from {
                match into.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (into, from) => *into = from,
    }
}

#[async_trait::async_trait]
impl CodingAgentLauncher for DaemonCodingAgentLauncher {
    /// Only the case where this daemon can become the session's user itself; a launch that needs
    /// the supervisor is refused here. [`Self::launch_transport`] is what the daemon opens a
    /// conversation through.
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        self.plan(launch)
    }

    async fn launch_transport(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AgentTransport, SubagentError> {
        let command = self.command_before_user_switch(launch)?;
        match run_as_session_user(command.clone(), &launch.session_os_user, &self.drop) {
            Ok(command) => Ok(AgentTransport::Command(command)),
            Err(AsUserRefusal::Unprivileged(reason)) => match &self.relay {
                Some(relay) => start_relayed(relay, &launch.session_os_user, &command)
                    .await
                    .map(AgentTransport::Stream)
                    .map_err(|e| Self::refuse(launch, &e.to_string())),
                None => Err(Self::refuse(launch, &reason)),
            },
            Err(other) => Err(Self::refuse(launch, &other.to_string())),
        }
    }

    async fn mcp_servers_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        if !launch.managed {
            return Ok(Vec::new());
        }
        Ok(vec![self.tool_server_spec(launch)?])
    }

    /// The headless command [`Self::headless_plan`] settles: refused unless the daemon can run the
    /// CLI exactly as the launch asks.
    async fn headless_command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        self.headless_plan(launch)
    }
}

/// The route resolver for a daemon whose vaults `access_for` reads: resolve the launch's provider
/// account in the session owner's vault, as a tool session does.
pub fn route_from_vault(
    access_for: impl Fn(&str) -> tddy_agent_launch::session_acting_identity::SessionAccountAccess
        + Send
        + Sync
        + 'static,
) -> RouteResolver {
    Arc::new(move |launch| {
        let Some(account) = launch.def.provider_account.clone() else {
            return Ok(None);
        };
        let harness =
            Harness::from_agent_or_session_type(&launch.def.harness).ok_or_else(|| {
                SubagentError::from(format!(
                    "'{}' is not a routable harness",
                    launch.def.harness
                ))
            })?;
        let sources = RouteSources {
            session: Some(RouteChoice::Account(account)),
            ..RouteSources::default()
        };
        tddy_agent_launch::session_provider_route::resolve_session_route(
            &access_for(&launch.session_token),
            harness,
            &sources,
            &launch.def.model,
        )
        .map_err(|status| SubagentError::from(status.message().to_string()))
    })
}

#[cfg(all(test, unix))]
mod headless_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_coding_subagent::CodingAgentDef;
    use tddy_core::provider_route::RouteSecret;

    /// The model every launch in these tests runs.
    const MODEL: &str = "gpt-5.2";
    /// The provider's endpoint the route points the agent at.
    const BASE_URL: &str = "https://llm.example.internal";
    /// The route's secret, which must reach the agent's environment.
    const API_KEY: &str = "sk-test-route-secret";
    /// Where the launches' tool server resolves `tddy-tools` to.
    const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";
    /// The session every launch serves.
    const SESSION_ID: &str = "1780828020298-coding";

    fn the_daemons_own_identity() -> DropContext {
        #[cfg(unix)]
        let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
        #[cfg(not(unix))]
        let (uid, gid) = (0, 0);
        DropContext {
            uid,
            gid,
            privileged: false,
            setpriv: None,
        }
    }

    fn an_unprivileged_identity() -> DropContext {
        DropContext {
            uid: 4242,
            gid: 4242,
            privileged: false,
            setpriv: None,
        }
    }

    fn this_user() -> String {
        #[cfg(unix)]
        {
            tddy_session_activity::user_sessions_path::username_for_uid(unsafe { libc::geteuid() })
                .expect("the test user has a passwd entry")
        }
        #[cfg(not(unix))]
        {
            String::new()
        }
    }

    fn a_launch(harness: &str) -> CodingAgentLaunch {
        CodingAgentLaunch {
            def: CodingAgentDef {
                name: "fix-the-parser".to_string(),
                label: None,
                harness: harness.to_string(),
                model: MODEL.to_string(),
                provider_account: None,
            },
            session_id: SESSION_ID.to_string(),
            session_os_user: this_user(),
            worktree: PathBuf::from("/home/dev/worktrees/parser-fix"),
            managed: false,
            session_token: "caller-session-token".to_string(),
            conversation_id: None,
        }
    }

    fn a_route() -> HarnessRoute {
        HarnessRoute {
            env: vec![("BASE_URL".to_string(), BASE_URL.to_string())],
            secret_env: vec![("API_KEY".to_string(), RouteSecret::new(API_KEY))],
            files: vec![tddy_core::provider_route::RouteFile {
                name: "opencode.json".to_string(),
                contents: r#"{"provider":{"internal-llm":{}}}"#.to_string(),
            }],
            args: vec!["-m".to_string(), MODEL.to_string()],
        }
    }

    fn a_launcher() -> DaemonCodingAgentLauncher {
        a_launcher_routed_by(Arc::new(|_| Ok(Some(a_route()))))
    }

    fn a_launcher_routed_by(route: RouteResolver) -> DaemonCodingAgentLauncher {
        DaemonCodingAgentLauncher::with_identity(
            the_daemons_own_identity(),
            route,
            Arc::new(|launch| {
                Ok((
                    PathBuf::from(TDDY_TOOLS_PATH),
                    vec![(
                        "TDDY_REMOTE_SESSION_ID".to_string(),
                        launch.session_id.clone(),
                    )],
                ))
            }),
        )
    }

    async fn command(
        launcher: &DaemonCodingAgentLauncher,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, String> {
        launcher
            .command_for(launch)
            .await
            .map_err(|e| e.to_string())
    }

    fn env_of<'a>(command: &'a AcpAgentCommand, name: &str) -> Option<&'a str> {
        command
            .env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn inline_opencode_config(command: &AcpAgentCommand) -> serde_json::Value {
        serde_json::from_str(env_of(command, OPENCODE_CONFIG_CONTENT).expect("inline config"))
            .expect("JSON")
    }

    async fn managed_claude_servers() -> Vec<McpServerSpec> {
        let mut launch = a_launch(CLAUDE_ACP);
        launch.managed = true;
        a_launcher()
            .mcp_servers_for(&launch)
            .await
            .expect("servers")
    }

    #[tokio::test]
    async fn a_session_of_the_daemons_own_user_starts_opencode_acp() {
        // Given
        let launch = a_launch(OPENCODE);

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(
            (command.program, command.args),
            (PathBuf::from("opencode"), vec!["acp".to_string()])
        );
    }

    #[tokio::test]
    async fn a_session_of_the_daemons_own_user_starts_the_harness_on_the_worktree() {
        // Given
        let launch = a_launch(OPENCODE);

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(
            command.cwd,
            Some(PathBuf::from("/home/dev/worktrees/parser-fix"))
        );
    }

    #[test]
    fn claude_runs_through_bunx_when_no_claude_agent_acp_binary_is_named() {
        // When
        let (program, args) = claude_agent_acp_command(None);

        // Then
        assert_eq!(
            (program, args),
            (PathBuf::from("bunx"), vec!["claude-agent-acp".to_string()])
        );
    }

    #[test]
    fn a_named_claude_agent_acp_binary_runs_on_its_own() {
        // Given
        let named = OsString::from("/opt/claude/bin/claude-agent-acp");

        // When
        let (program, args) = claude_agent_acp_command(Some(named));

        // Then
        assert_eq!(
            (program, args),
            (
                PathBuf::from("/opt/claude/bin/claude-agent-acp"),
                Vec::new()
            )
        );
    }

    #[tokio::test]
    async fn a_session_of_another_user_is_refused_by_an_unprivileged_daemon() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.session_os_user = "root".to_string();
        let launcher = DaemonCodingAgentLauncher::with_identity(
            an_unprivileged_identity(),
            Arc::new(|_| Ok(None)),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        );

        // When
        let refusal = command(&launcher, &launch).await.expect_err("refused");

        // Then
        assert!(
            refusal.contains("'root'") && refusal.contains("not privileged"),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn a_provider_route_reaches_the_codex_agent_as_env() {
        // Given
        let mut launch = a_launch(CODEX_ACP);
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(env_of(&command, "BASE_URL"), Some(BASE_URL));
    }

    #[tokio::test]
    async fn a_provider_route_reaches_the_codex_agent_as_secret_env() {
        // Given
        let mut launch = a_launch(CODEX_ACP);
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(env_of(&command, "API_KEY"), Some(API_KEY));
    }

    #[tokio::test]
    async fn a_provider_route_reaches_the_codex_agent_as_overrides() {
        // Given
        let mut launch = a_launch(CODEX_ACP);
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert!(
            command
                .args
                .windows(2)
                .any(|w| w == ["-c", "model=\"gpt-5.2\""]),
            "{:?}",
            command.args
        );
    }

    #[tokio::test]
    async fn a_provider_route_reaches_opencode_as_inline_config() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(
            inline_opencode_config(&command)["provider"]["internal-llm"],
            serde_json::json!({})
        );
    }

    #[tokio::test]
    async fn a_route_that_does_not_resolve_refuses_rather_than_run_on_the_vendor() {
        // Given
        let mut launch = a_launch(CLAUDE_ACP);
        launch.def.provider_account = Some("internal-llm".to_string());
        let launcher = a_launcher_routed_by(Arc::new(|_| {
            Err(SubagentError::from("the vault is locked"))
        }));

        // When
        let refusal = command(&launcher, &launch).await.expect_err("refused");

        // Then
        assert!(refusal.contains("vault is locked"), "{refusal}");
    }

    #[tokio::test]
    async fn a_managed_opencode_loses_its_native_tools() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.managed = true;
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(inline_opencode_config(&command)["tools"]["bash"], false);
    }

    #[tokio::test]
    async fn a_managed_opencode_keeps_its_route() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.managed = true;
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        let command = command(&a_launcher(), &launch).await.expect("launches");

        // Then
        assert_eq!(
            inline_opencode_config(&command)["provider"]["internal-llm"],
            serde_json::json!({})
        );
    }

    #[tokio::test]
    async fn an_inline_opencode_config_that_is_not_json_is_refused_naming_it() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.managed = true;
        launch.def.provider_account = Some("internal-llm".to_string());
        let launcher = a_launcher_routed_by(Arc::new(|_| {
            let mut route = a_route();
            route.files.clear();
            route.env.push((
                OPENCODE_CONFIG_CONTENT.to_string(),
                "provider = internal-llm".to_string(),
            ));
            Ok(Some(route))
        }));

        // When
        let refusal = command(&launcher, &launch).await.expect_err("refused");

        // Then
        assert!(
            refusal.contains("OPENCODE_CONFIG_CONTENT") && refusal.contains("not JSON"),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn a_managed_harness_that_cannot_withdraw_is_refused() {
        // Given
        let mut launch = a_launch(CODEX_ACP);
        launch.managed = true;

        // When
        let refusal = command(&a_launcher(), &launch).await.expect_err("refused");

        // Then
        assert!(refusal.contains("cannot be withdrawn"), "{refusal}");
    }

    #[tokio::test]
    async fn a_managed_launch_hands_the_agent_one_mcp_server_named_tddy_tools() {
        // When
        let servers = managed_claude_servers().await;

        // Then
        let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["tddy-tools"]);
    }

    #[tokio::test]
    async fn a_managed_launch_runs_tddy_tools_as_an_mcp_server() {
        // When
        let servers = managed_claude_servers().await;

        // Then
        assert_eq!(
            (servers[0].command.clone(), servers[0].args.clone()),
            (PathBuf::from(TDDY_TOOLS_PATH), vec!["--mcp".to_string()])
        );
    }

    #[tokio::test]
    async fn a_managed_launch_hands_tddy_tools_its_transport() {
        // When
        let servers = managed_claude_servers().await;

        // Then
        assert_eq!(
            servers[0].env,
            [("TDDY_REMOTE_SESSION_ID".to_string(), SESSION_ID.to_string())]
        );
    }

    #[tokio::test]
    async fn an_unmanaged_launch_gets_no_mcp_servers() {
        // When
        let servers = a_launcher()
            .mcp_servers_for(&a_launch(OPENCODE))
            .await
            .expect("servers");

        // Then
        assert!(servers.is_empty());
    }

    struct RecordingSupervisor {
        asked: std::sync::Mutex<Vec<tddy_supervisor::SpawnRelayedRequest>>,
    }

    #[async_trait::async_trait]
    impl crate::coding_agent_relay::RelaySpawner for RecordingSupervisor {
        async fn spawn_relayed(
            &self,
            request: tddy_supervisor::SpawnRelayedRequest,
        ) -> Result<tddy_supervisor::SpawnedProcess, tddy_supervisor::SupervisorError> {
            self.asked.lock().expect("asked").push(request.clone());
            let _stream = std::os::unix::net::UnixStream::connect(&request.relay_socket)
                .expect("the supervisor connects");
            Ok(tddy_supervisor::SpawnedProcess { pid: 7 })
        }
    }

    impl RecordingSupervisor {
        fn asked(&self) -> Vec<tddy_supervisor::SpawnRelayedRequest> {
            self.asked.lock().expect("asked").clone()
        }
    }

    fn an_unprivileged_daemon_with_a_supervisor() -> (
        DaemonCodingAgentLauncher,
        Arc<RecordingSupervisor>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let supervisor = Arc::new(RecordingSupervisor {
            asked: std::sync::Mutex::new(Vec::new()),
        });
        let launcher = DaemonCodingAgentLauncher::with_identity(
            an_unprivileged_identity(),
            Arc::new(|_| Ok(None)),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        )
        .with_supervisor_relay(RelayEndpoint {
            spawner: supervisor.clone(),
            socket_dir: dir.path().join("r"),
        });
        (launcher, supervisor, dir)
    }

    fn a_launch_for_root() -> CodingAgentLaunch {
        let mut launch = a_launch(OPENCODE);
        launch.session_os_user = "root".to_string();
        launch
    }

    #[tokio::test]
    async fn an_unprivileged_daemon_reaches_another_users_agent_over_a_relayed_stream() {
        // Given
        let (launcher, _supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();

        // When
        let transport = launcher
            .launch_transport(&a_launch_for_root())
            .await
            .expect("transport");

        // Then
        assert!(matches!(transport, AgentTransport::Stream(_)));
    }

    #[tokio::test]
    async fn an_unprivileged_daemon_asks_the_supervisor_for_the_session_user() {
        // Given
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();

        // When
        launcher
            .launch_transport(&a_launch_for_root())
            .await
            .expect("transport");

        // Then
        let users: Vec<String> = supervisor.asked().into_iter().map(|r| r.os_user).collect();
        assert_eq!(users, ["root"]);
    }

    #[tokio::test]
    async fn an_unprivileged_daemon_asks_the_supervisor_for_the_harness_on_the_worktree() {
        // Given
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();

        // When
        launcher
            .launch_transport(&a_launch_for_root())
            .await
            .expect("transport");

        // Then
        let asked = &supervisor.asked()[0];
        assert_eq!(
            (asked.args.clone(), asked.working_dir.clone()),
            (
                vec!["acp".to_string()],
                Some(PathBuf::from("/home/dev/worktrees/parser-fix"))
            )
        );
    }

    #[tokio::test]
    async fn the_daemons_own_user_needs_no_supervisor() {
        // Given
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();
        let launcher = DaemonCodingAgentLauncher {
            drop: the_daemons_own_identity(),
            ..launcher
        };

        // When
        let transport = launcher
            .launch_transport(&a_launch(OPENCODE))
            .await
            .expect("transport");

        // Then
        assert!(matches!(transport, AgentTransport::Command(_)));
        assert!(supervisor.asked().is_empty());
    }

    #[tokio::test]
    async fn a_supervised_launch_still_applies_the_route_it_sends_as_environment() {
        // Given
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();
        let launcher = DaemonCodingAgentLauncher {
            route: Arc::new(|_| Ok(Some(a_route()))),
            ..launcher
        };
        let mut launch = a_launch(CLAUDE_ACP);
        launch.session_os_user = "root".to_string();
        launch.def.provider_account = Some("internal-llm".to_string());

        // When
        launcher.launch_transport(&launch).await.expect("transport");

        // Then
        assert_eq!(
            supervisor.asked()[0].env.get("API_KEY").map(String::as_str),
            Some(API_KEY)
        );
    }

    #[tokio::test]
    async fn a_harness_with_no_command_is_refused() {
        // Given
        let launch = a_launch("claude");

        // When
        let refusal = command(&a_launcher(), &launch).await.expect_err("refused");

        // Then
        assert!(refusal.contains("'claude' harness"), "{refusal}");
    }
}
