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
//!   (the spec's `acp.withdraw_native`: inline config for `opencode`, `_meta` at `session/new` for
//!   `claude-acp`) and
//!   the agent is handed `tddy-tools --mcp` with a credential minted for it
//!   ([`tddy_session_split::coding_agent_wiring`]).
//!
//! **A headless harness** (`claude`, `cursor`, `codex`, `oz`) runs its CLI once per turn rather than
//! speaking ACP: [`DaemonCodingAgentLauncher::headless_command_for`] names that CLI, the binary its
//! tool definition resolves, on the conversation's worktree, with the route's environment. It runs
//! only as the daemon's own user, so a session of another OS user is refused. Every headless harness
//! is planned from its tool definition: a `claude` conversation is handed `tddy-tools --mcp` through
//! an owner-only MCP config under the daemon's data directory, its permission prompts routed to that
//! server, and Cursor, Codex and Oz are handed the server as their MCP slots declare — a worktree `.cursor/mcp.json`, inline `-c` overrides, an
//! `--mcp` file — with what the conversation will write recorded for its close and the sweep. A
//! daemon whose coding agents did not load, and a launcher built without any, refuse every headless
//! harness: nothing is planned by hand.
//!
//! **The harness binary** is the agent spec's: `claude-acp` runs as the spec declares (`bunx
//! claude-agent-acp` unless `TDDY_CLAUDE_AGENT_ACP_CLI` names a binary), `codex-acp` and `opencode`
//! likewise. On a supervised host the program is what `allowed_relay_tool_paths` must list, and its
//! arguments are not allowlisted — so allowlisting `bunx` lets the session user run any npm package;
//! set `TDDY_CLAUDE_AGENT_ACP_CLI` and allowlist that binary instead.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_agent_launch::host_coding_agents::host_coding_agents;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::slots::headless_mcp::mcp_written_paths;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_coding_subagent::launcher::{record_written_files, HEADLESS_DIR, SESSION_TOKEN_ENV};
#[cfg(test)]
use tddy_coding_subagent::CODEX_ACP;
#[cfg(test)]
use tddy_coding_subagent::OPENCODE;
#[cfg(test)]
use tddy_coding_subagent::OPENCODE_CONFIG_CONTENT;
use tddy_coding_subagent::{
    refuse_unless_confinable, AgentTransport, ApprovalRoutes, CodingAgentLaunch,
    CodingAgentLauncher, CONVERSATION_ID_ENV, TDDY_TOOLS_MCP_ARG, TDDY_TOOLS_SERVER,
};
use tddy_core::provider_route::{HarnessRoute, RouteChoice, RouteSources};
use tddy_core::{AcpAgentCommand, HeadlessCommand, McpServerSpec};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_base_instance_id_for_config;
use tddy_discovery::subagent::SubagentError;
use tddy_subagent_worktree::ConversationId;

use crate::coding_agent_as_user::{run_as_session_user, AsUserRefusal, DropContext};
use crate::coding_agent_relay::{start_relayed, RelayEndpoint};

/// The directory under `<tddyhome>` a supervised launch's relay sockets are bound in.
const RELAY_SOCKET_DIR: &str = "relay";
/// The tool Claude Code's permission prompts are routed to: `approval_prompt` on the `tddy-tools`
/// server (`mcp__<server>__<tool>`).
const APPROVAL_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";

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

/// What a headless conversation is launched with: the directory a conversation keeps the MCP config
/// its CLI reads in.
#[derive(Clone)]
struct HeadlessWiring {
    data_dir: PathBuf,
    /// This daemon's durable instance id, which every conversation directory it creates is tagged
    /// with, so its start-up sweep removes only its own.
    owner: String,
}

/// A headless conversation's directory, created for a plan still in progress: removed when dropped,
/// unless the plan kept it.
struct ClaimedFiles {
    dir: PathBuf,
    kept: bool,
}

impl ClaimedFiles {
    fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for ClaimedFiles {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(&self.dir) {
            log::warn!(
                "could not remove the files of a headless launch that failed, at {}: {e}",
                self.dir.display()
            );
        }
    }
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
    /// The coding agents declared as tools (`#agent-tools 6/12`): with them, every headless harness
    /// is rendered from its spec's `coding_agent.headless` section (`#agent-tools 15/19`: a launcher
    /// without them refuses a headless launch — nothing is planned by hand), and, since
    /// `#agent-tools 11/12`, the ACP harnesses are planned from their specs too (`acp_plan_from_spec`).
    ///
    /// Always present (`#agent-tools 19/19`): a launcher built without any holds an empty registry,
    /// which refuses every agent launch for naming no spec — nothing is planned by hand.
    coding_agents: Arc<HostCodingAgents>,
    /// Why `for_daemon`'s coding agents did not load. A headless `claude` conversation is refused with
    /// it, never planned by hand: a tool file that does not load must not start the agent some other way.
    coding_agents_error: Option<String>,
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
    /// this launcher: a conversation keeps its MCP config under `data_dir`, tagged with the
    /// instance id `config` names.
    #[must_use]
    pub fn with_headless_wiring(mut self, config: DaemonConfig, data_dir: PathBuf) -> Self {
        let owner = local_base_instance_id_for_config(&config);
        self.headless = Some(HeadlessWiring { data_dir, owner });
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
        let coding_agents = host_coding_agents(&tddy_data_dir);
        let supervisor_socket = match spawn_backend {
            tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor { socket_path } => {
                Some(socket_path)
            }
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => None,
        };
        let route = route_from_vault(
            move |token| {
                tddy_agent_launch::session_acting_identity::SessionAccountAccess::new(
                    vaults.clone(),
                    Arc::clone(&user_resolver),
                    token,
                )
            },
            coding_agents.as_ref().ok().cloned(),
        );
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
        let mut launcher =
            Self::new(route, tool_server).with_headless_wiring(headless_config, headless_dir);
        match coding_agents {
            Ok(agents) => launcher = launcher.with_coding_agents(agents),
            Err(error) => launcher.coding_agents_error = Some(format!("{error:#}")),
        }
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
            coding_agents: tddy_agent_launch::host_coding_agents::no_coding_agents(),
            coding_agents_error: None,
        }
    }

    /// Plan headless `claude` conversations from `agents`' declaration of it (builder): its binary
    /// resolved by the agent registry's host, each turn's argv rendered from
    /// `coding_agent.headless`, and its MCP registration and approval tool from its slots.
    #[must_use]
    pub fn with_coding_agents(mut self, agents: Arc<HostCodingAgents>) -> Self {
        self.coding_agents = agents;
        self
    }

    fn refuse(launch: &CodingAgentLaunch, reason: &str) -> SubagentError {
        SubagentError::from(format!(
            "coding agent '{}' ({}) cannot be launched: {reason}",
            launch.def.name, launch.def.harness
        ))
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
    pub(super) fn command_before_user_switch(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        let agents = self.acp_agents_for(launch)?;
        self.acp_plan_from_spec(launch, agents)
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
            cwd: None,
        })
    }

    /// The command a headless conversation of `launch` runs its CLI with. Refused rather than started
    /// some weaker way: a launch the daemon cannot honour exactly is named, not run.
    pub(super) fn headless_plan(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        let wiring = self.headless.as_ref().ok_or_else(|| {
            Self::refuse(launch, "this daemon has no headless coding-agent wiring")
        })?;
        refuse_unless_confinable(&launch.def.harness, launch.managed)
            .map_err(|reason| Self::refuse(launch, &reason))?;
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
        // `#agent-tools 9/12`: every headless harness — Cursor, Codex and Oz as well as Claude — is
        // planned from its spec once the coding agents are installed (`cursor.yaml`, `codex.yaml`,
        // `oz.yaml`).
        // A tool file that does not load must not start any agent some other way.
        if let Some(reason) = &self.coding_agents_error {
            return Err(Self::refuse(launch, reason));
        }
        // Nothing is planned by hand: an agent no installed spec declares has no argv to start a
        // headless CLI with (`agents.resolve` says so).
        self.headless_plan_from_spec(launch, wiring, conversation.as_str(), &self.coding_agents)
    }

    /// Records, in the claimed directory `scratch_dir`, every file the turns of `spec`'s headless
    /// MCP slot (`headless.mcp`, else `coding_agent.mcp`) will write, so the close and the start-up
    /// sweep find what lies outside it. Before anything is written, a file the slot places in the
    /// worktree (`.cursor/mcp.json`) is refused when one is already there — it may be the
    /// repository's own, and tracked — and kept out of git, its contents carrying the agent's tool
    /// credential.
    fn record_written_files(
        &self,
        launch: &CodingAgentLaunch,
        spec: &CodingAgentSpec,
        scratch_dir: &Path,
    ) -> Result<(), SubagentError> {
        let slot = spec
            .headless()
            .and_then(|headless| headless.mcp.as_ref())
            .unwrap_or(&spec.agent.mcp);
        let written = mcp_written_paths(slot, &launch.worktree, scratch_dir, None)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        for path in &written {
            let Ok(relative) = path.strip_prefix(&launch.worktree) else {
                continue;
            };
            if path.symlink_metadata().is_ok() {
                return Err(Self::refuse(
                    launch,
                    &format!(
                        "its worktree already has a {}, which is not overwritten",
                        relative.display()
                    ),
                ));
            }
            // Ignored before it is written, so the credential is never an untracked file git would
            // offer.
            tddy_core::turn_files::exclude_from_git(
                &launch.worktree,
                &format!("/{}", relative.display()),
            )
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        }
        record_written_files(scratch_dir, &written).map_err(|e| {
            Self::refuse(
                launch,
                &format!("the files its turns will write could not be recorded: {e}"),
            )
        })
    }

    /// Create the directory conversation `conversation`'s files are kept in, tagged with this daemon,
    /// removed again unless the plan [`ClaimedFiles::keep`]s it.
    fn claim_conversation_files(
        &self,
        launch: &CodingAgentLaunch,
        wiring: &HeadlessWiring,
        conversation: &str,
    ) -> Result<ClaimedFiles, SubagentError> {
        let dir = wiring.data_dir.join(conversation);
        tddy_coding_subagent::launcher::claim_headless_conversation_dir(&dir, &wiring.owner)
            .map_err(|e| {
                Self::refuse(
                    launch,
                    &format!(
                        "its conversation's files could not be written under {}: {e}",
                        dir.display()
                    ),
                )
            })?;
        Ok(ClaimedFiles { dir, kept: false })
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

/// Remove the files the headless conversations of this daemon's previous run left under
/// `<tddy_data_dir>/headless/` — each holds its agent's tool credential. Run once at start-up, before
/// the daemon serves: it holds no conversation then. The daemon is named by its durable instance id
/// (the one that survives a restart), so a daemon sharing the data directory under another id keeps
/// its own. Returns the conversations whose files were removed.
pub fn sweep_headless_files_left_by_this_daemon(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
) -> io::Result<Vec<String>> {
    tddy_coding_subagent::launcher::sweep_headless_dirs(
        tddy_data_dir,
        &local_base_instance_id_for_config(config),
    )
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

/// The environment a provider route gives a harness's process: its plain pairs, then its secret ones.
fn harness_route_env(route: &HarnessRoute) -> Vec<(String, String)> {
    route
        .env
        .iter()
        .cloned()
        .chain(
            route
                .secret_env
                .iter()
                .map(|(name, secret)| (name.clone(), secret.expose().to_string())),
        )
        .collect()
}

/// Set `config` as the inline JSON configuration in variable `env` (OpenCode's
/// `OPENCODE_CONFIG_CONTENT`), merged over what the command already sets there. An existing value
/// that is not JSON is refused naming it, never silently replaced.
fn merge_inline_config(
    command: &mut AcpAgentCommand,
    env: &str,
    config: serde_json::Value,
) -> Result<(), String> {
    let existing = match command.env.iter().position(|(name, _)| name == env) {
        Some(at) => serde_json::from_str::<serde_json::Value>(&command.env[at].1)
            .map(|parsed| {
                command.env.remove(at);
                Some(parsed)
            })
            .map_err(|e| format!("the {env} it already sets is not JSON: {e}"))?,
        None => None,
    };
    let mut merged = existing.unwrap_or_else(|| serde_json::json!({}));
    merge_json(&mut merged, config);
    command.env.push((env.to_string(), merged.to_string()));
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
        let agents = self.acp_agents_for(launch)?;
        self.acp_mcp_servers_from_spec(launch, agents)
    }

    async fn session_meta_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, SubagentError> {
        let agents = self.acp_agents_for(launch)?;
        self.acp_session_meta_from_spec(launch, agents)
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
/// account in the session owner's vault, as a tool session does. The launch's agent is read from
/// `agents`' registry: a host with none, or an agent it does not know, refuses.
pub fn route_from_vault(
    access_for: impl Fn(&str) -> tddy_agent_launch::session_acting_identity::SessionAccountAccess
        + Send
        + Sync
        + 'static,
    agents: Option<Arc<HostCodingAgents>>,
) -> RouteResolver {
    Arc::new(move |launch| {
        let Some(account) = launch.def.provider_account.clone() else {
            return Ok(None);
        };
        let agent = tddy_agent_launch::session_provider_route::routed_agent_on_host(
            agents.as_deref(),
            &launch.def.harness,
        )
        .map_err(|status| SubagentError::from(status.message().to_string()))?;
        let sources = RouteSources {
            session: Some(RouteChoice::Account(account)),
            ..RouteSources::default()
        };
        tddy_agent_launch::session_provider_route::resolve_session_route(
            &access_for(&launch.session_token),
            &agent,
            &sources,
            &launch.def.model,
        )
        .map_err(|status| SubagentError::from(status.message().to_string()))
    })
}

mod acp;
mod headless;
#[cfg(all(test, unix))]
mod headless_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_coding_subagent::{CodingAgentDef, CLAUDE_ACP};
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
        .with_coding_agents(the_acp_agents())
    }

    /// The directory holding the stub binaries [`the_acp_agents`] resolves to, made once and
    /// canonical, as the host resolves a binary (`/var` is `/private/var` on macOS).
    fn the_stubs() -> &'static Path {
        use std::os::unix::fs::PermissionsExt;
        static STUBS: std::sync::OnceLock<(tempfile::TempDir, PathBuf)> =
            std::sync::OnceLock::new();
        &STUBS
            .get_or_init(|| {
                let dir = tempfile::tempdir().expect("a directory for the stub agents");
                let canonical = std::fs::canonicalize(dir.path()).expect("a canonical path");
                for name in [
                    "claude-agent-acp",
                    "codex-acp",
                    "codex",
                    "opencode",
                    "tddy-tools",
                ] {
                    let file = canonical.join(name);
                    std::fs::write(&file, "#!/bin/sh\nexit 0\n").expect("a stub agent");
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                        .expect("an executable stub agent");
                }
                (dir, canonical)
            })
            .1
    }

    /// The ACP coding agents of a host whose binaries are the stubs ([`the_stubs`]).
    fn the_acp_agents() -> Arc<HostCodingAgents> {
        let stub = |name: &str| the_stubs().join(name);
        let (claude, codex_acp, codex, opencode, tddy_tools) = (
            stub("claude-agent-acp"),
            stub("codex-acp"),
            stub("codex"),
            stub("opencode"),
            stub("tddy-tools"),
        );
        crate::test_util::coding_agents_with_binaries(&[
            ("claude-acp", claude.as_path()),
            ("codex-acp", codex_acp.as_path()),
            ("codex", codex.as_path()),
            ("opencode", opencode.as_path()),
            ("tddy-tools", tddy_tools.as_path()),
        ])
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
            (the_stubs().join("opencode"), vec!["acp".to_string()])
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

    #[tokio::test]
    async fn a_session_of_another_user_is_refused_by_an_unprivileged_daemon() {
        // Given
        let mut launch = a_launch(OPENCODE);
        launch.session_os_user = "root".to_string();
        let launcher = DaemonCodingAgentLauncher::with_identity(
            an_unprivileged_identity(),
            Arc::new(|_| Ok(None)),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        )
        .with_coding_agents(the_acp_agents());

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
        assert!(refusal.contains("cannot withdraw its native"), "{refusal}");
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
        .with_coding_agents(the_acp_agents())
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
