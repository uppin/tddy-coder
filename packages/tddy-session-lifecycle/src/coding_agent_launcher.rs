//! The daemon's [`CodingAgentLauncher`]: turns a coding-agent launch into the ACP agent command.
//!
//! A coding agent is a subagent that can write, so a launch the daemon cannot honour *exactly* is
//! refused with a typed reason rather than started some weaker way:
//!
//! - **The session's OS user** — [`crate::coding_agent_as_user`]: run behind `setpriv` when this
//!   daemon is privileged; refused when it is not (a supervised host: `tddy-supervisor`'s
//!   `SpawnSession` gives a child no stdio, so the ACP driver cannot talk to it).
//!   FIXME(acp-subagent): a supervisor request that relays a child's stdio.
//! - **A provider account's route** — resolved from the session owner's vault through
//!   [`tddy_agent_launch::session_provider_route::resolve_session_route`], exactly as a tool session
//!   resolves it, and applied per harness: environment and secret environment for all of them,
//!   `-c` overrides for `codex-acp`, the route's config file as inline `OPENCODE_CONFIG_CONTENT` for
//!   `opencode`.
//! - **A managed session** — the harness's native tools are withdrawn
//!   ([`tddy_coding_subagent::withdrawal_env`]; `claude-acp` through `_meta` at `session/new`) and
//!   the agent is handed `tddy-tools --mcp` with a credential minted for it
//!   ([`tddy_session_split::coding_agent_wiring`]).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

use std::path::PathBuf;
use std::sync::Arc;

use tddy_coding_subagent::{
    confinement_for, withdrawal_env, AgentTransport, CodingAgentLaunch, CodingAgentLauncher,
    Confinement,
};
use tddy_core::provider_route::{Harness, HarnessRoute, RouteChoice, RouteSources};
use tddy_core::{AcpAgentCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

use crate::coding_agent_as_user::{run_as_session_user, AsUserRefusal, DropContext};
use crate::coding_agent_relay::{start_relayed, RelayEndpoint};

/// Env var naming the `codex-acp` binary, as the workflow backend reads it.
const CODEX_ACP_CLI_ENV: &str = "TDDY_CODEX_ACP_CLI";
/// The variable `opencode` reads inline JSON configuration from.
const OPENCODE_CONFIG_CONTENT_ENV: &str = "OPENCODE_CONFIG_CONTENT";

/// Resolves the provider route a launch runs on: `None` for the harness's own login.
pub type RouteResolver =
    Arc<dyn Fn(&CodingAgentLaunch) -> Result<Option<HarnessRoute>, SubagentError> + Send + Sync>;

/// The `tddy-tools` binary and environment a managed launch's MCP server runs with.
pub type ToolServerResolver = Arc<
    dyn Fn(&CodingAgentLaunch) -> Result<(PathBuf, Vec<(String, String)>), SubagentError>
        + Send
        + Sync,
>;

/// Starts the harness a launch names, as the session's OS user, on the session's worktree.
#[derive(Clone)]
pub struct DaemonCodingAgentLauncher {
    drop: DropContext,
    route: RouteResolver,
    tool_server: ToolServerResolver,
    /// How an unprivileged daemon starts an agent as another user: through `tddy-supervisor`.
    relay: Option<RelayEndpoint>,
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

    /// The launcher a running daemon installs: routes from the session owner's vault, tool servers
    /// from this daemon's own tokens and toolchain.
    #[must_use]
    pub fn for_daemon(
        config: tddy_daemon_kernel::config::DaemonConfig,
        tddy_data_dir: PathBuf,
        vaults: Option<Arc<tddy_daemon_auth::SessionVaults>>,
        user_resolver: tddy_daemon_kernel::SessionUserResolver,
        tokens: Option<tddy_daemon_auth::SessionTokens>,
    ) -> Self {
        let relay_dir = tddy_data_dir.join("relay");
        let supervisor_socket = config.supervisor.as_ref().map(|s| s.socket_path.clone());
        let route = route_from_vault(move |token| {
            tddy_agent_launch::session_acting_identity::SessionAccountAccess::new(
                vaults.clone(),
                Arc::clone(&user_resolver),
                token,
            )
        });
        let tool_server: ToolServerResolver = Arc::new(move |launch| {
            tddy_session_split::coding_agent_wiring::coding_agent_tool_server(
                &config,
                &tddy_data_dir,
                tokens.as_ref(),
                &launch.session_id,
                &launch.session_token,
            )
            .map_err(|status| SubagentError::from(status.message().to_string()))
        });
        let launcher = Self::new(route, tool_server);
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
            "claude-acp" => Ok((PathBuf::from("bunx"), vec!["claude-agent-acp".to_string()])),
            "codex-acp" => Ok((
                std::env::var_os(CODEX_ACP_CLI_ENV)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("codex-acp")),
                Vec::new(),
            )),
            "opencode" => Ok((PathBuf::from("opencode"), vec!["acp".to_string()])),
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
            "codex-acp" => command.args.extend(tddy_core::codex_acp_route_args(route)),
            "opencode" => {
                if let Some(file) = route.files.first() {
                    let config: serde_json::Value =
                        serde_json::from_str(&file.contents).map_err(|e| {
                            Self::refuse(
                                launch,
                                &format!("the route's OpenCode config is not JSON: {e}"),
                            )
                        })?;
                    set_opencode_config(command, config);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The command for `launch` as the harness runs it, before it is made to run as another user.
    fn command_before_user_switch(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        let (program, args) = Self::harness_command(launch)?;
        let mut command = AcpAgentCommand {
            program,
            args,
            env: Vec::new(),
            cwd: Some(launch.worktree.clone()),
        };
        if launch.managed && confinement_for(&launch.def.harness) == Confinement::CannotWithdraw {
            return Err(Self::refuse(
                launch,
                "it is a managed session and this harness's native tools cannot be withdrawn",
            ));
        }
        if let Some(account) = launch.def.provider_account.as_deref() {
            let route = (self.route)(launch)?.ok_or_else(|| {
                Self::refuse(
                    launch,
                    &format!("provider account '{account}' resolved to no route"),
                )
            })?;
            Self::apply_route(launch, &mut command, &route)?;
        }
        if launch.managed {
            for (name, value) in withdrawal_env(&launch.def.harness) {
                if name == OPENCODE_CONFIG_CONTENT_ENV {
                    let tools = serde_json::from_str(&value).map_err(|e| {
                        Self::refuse(launch, &format!("the withdrawal config is not JSON: {e}"))
                    })?;
                    set_opencode_config(&mut command, tools);
                } else {
                    command.env.push((name, value));
                }
            }
        }
        Ok(command)
    }

    fn plan(&self, launch: &CodingAgentLaunch) -> Result<AcpAgentCommand, SubagentError> {
        let command = self.command_before_user_switch(launch)?;
        run_as_session_user(command, &launch.session_os_user, &self.drop)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))
    }
}

/// Merge `config` into the command's inline OpenCode config, creating it when absent.
fn set_opencode_config(command: &mut AcpAgentCommand, config: serde_json::Value) {
    let existing = command
        .env
        .iter()
        .position(|(name, _)| name == OPENCODE_CONFIG_CONTENT_ENV)
        .map(|at| command.env.remove(at).1)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let mut merged = existing.unwrap_or_else(|| serde_json::json!({}));
    merge_json(&mut merged, config);
    command
        .env
        .push((OPENCODE_CONFIG_CONTENT_ENV.to_string(), merged.to_string()));
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
            return Ok(launch.mcp_servers.clone());
        }
        let (command, env) = (self.tool_server)(launch)?;
        Ok(vec![McpServerSpec {
            name: "tddy-tools".to_string(),
            command,
            args: vec!["--mcp".to_string()],
            env,
        }])
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

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_coding_subagent::CodingAgentDef;
    use tddy_core::provider_route::RouteSecret;

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
                name: "coder".to_string(),
                label: None,
                harness: harness.to_string(),
                model: "m".to_string(),
                provider_account: None,
            },
            session_id: "s".to_string(),
            session_os_user: this_user(),
            worktree: PathBuf::from("/work/tree"),
            managed: false,
            mcp_servers: Vec::new(),
            session_token: "caller".to_string(),
        }
    }

    fn a_route() -> HarnessRoute {
        HarnessRoute {
            env: vec![("BASE_URL".to_string(), "https://p".to_string())],
            secret_env: vec![("API_KEY".to_string(), RouteSecret::new("sekret"))],
            files: vec![tddy_core::provider_route::RouteFile {
                name: "opencode.json".to_string(),
                contents: r#"{"provider":{"p":{}}}"#.to_string(),
            }],
            args: vec!["-m".to_string(), "m".to_string()],
        }
    }

    fn a_launcher() -> DaemonCodingAgentLauncher {
        DaemonCodingAgentLauncher::with_identity(
            the_daemons_own_identity(),
            Arc::new(|_| Ok(Some(a_route()))),
            Arc::new(|launch| {
                Ok((
                    PathBuf::from("/bin/tddy-tools"),
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

    #[tokio::test]
    async fn a_session_of_the_daemons_own_user_starts_the_harness_on_the_worktree() {
        let command = command(&a_launcher(), &a_launch("opencode"))
            .await
            .expect("launches");

        assert_eq!(command.program, PathBuf::from("opencode"));
        assert_eq!(command.args, ["acp"]);
        assert_eq!(command.cwd, Some(PathBuf::from("/work/tree")));
    }

    #[tokio::test]
    async fn a_session_of_another_user_is_refused_by_an_unprivileged_daemon() {
        let mut launch = a_launch("opencode");
        launch.session_os_user = "root".to_string();
        let launcher = DaemonCodingAgentLauncher::with_identity(
            DropContext {
                uid: 4242,
                gid: 4242,
                privileged: false,
                setpriv: None,
            },
            Arc::new(|_| Ok(None)),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        );

        let refusal = command(&launcher, &launch).await.expect_err("refused");

        assert!(
            refusal.contains("'root'") && refusal.contains("not privileged"),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn a_provider_route_reaches_the_codex_agent_as_env_and_overrides() {
        let mut launch = a_launch("codex-acp");
        launch.def.provider_account = Some("meta".to_string());

        let command = command(&a_launcher(), &launch).await.expect("launches");

        assert_eq!(env_of(&command, "BASE_URL"), Some("https://p"));
        assert_eq!(env_of(&command, "API_KEY"), Some("sekret"));
        assert!(
            command.args.windows(2).any(|w| w == ["-c", "model=\"m\""]),
            "{:?}",
            command.args
        );
    }

    #[tokio::test]
    async fn a_provider_route_reaches_opencode_as_inline_config() {
        let mut launch = a_launch("opencode");
        launch.def.provider_account = Some("meta".to_string());

        let command = command(&a_launcher(), &launch).await.expect("launches");

        let config: serde_json::Value =
            serde_json::from_str(env_of(&command, "OPENCODE_CONFIG_CONTENT").expect("config"))
                .expect("JSON");
        assert_eq!(config["provider"]["p"], serde_json::json!({}));
        assert_eq!(env_of(&command, "API_KEY"), Some("sekret"));
    }

    #[tokio::test]
    async fn a_route_that_does_not_resolve_refuses_rather_than_run_on_the_vendor() {
        let mut launch = a_launch("claude-acp");
        launch.def.provider_account = Some("meta".to_string());
        let launcher = DaemonCodingAgentLauncher::with_identity(
            the_daemons_own_identity(),
            Arc::new(|_| Err(SubagentError::from("the vault is locked"))),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        );

        let refusal = command(&launcher, &launch).await.expect_err("refused");

        assert!(refusal.contains("vault is locked"), "{refusal}");
    }

    #[tokio::test]
    async fn a_managed_opencode_loses_its_native_tools_and_keeps_its_route() {
        let mut launch = a_launch("opencode");
        launch.managed = true;
        launch.def.provider_account = Some("meta".to_string());

        let command = command(&a_launcher(), &launch).await.expect("launches");

        let config: serde_json::Value =
            serde_json::from_str(env_of(&command, "OPENCODE_CONFIG_CONTENT").expect("config"))
                .expect("JSON");
        assert_eq!(config["tools"]["bash"], false);
        assert_eq!(config["provider"]["p"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn a_managed_harness_that_cannot_withdraw_is_refused() {
        let mut launch = a_launch("codex-acp");
        launch.managed = true;

        let refusal = command(&a_launcher(), &launch).await.expect_err("refused");

        assert!(refusal.contains("cannot be withdrawn"), "{refusal}");
    }

    #[tokio::test]
    async fn a_managed_launch_hands_the_agent_tddy_tools_with_its_transport() {
        let mut launch = a_launch("claude-acp");
        launch.managed = true;

        let servers = a_launcher()
            .mcp_servers_for(&launch)
            .await
            .expect("servers");

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "tddy-tools");
        assert_eq!(servers[0].args, ["--mcp"]);
        assert_eq!(servers[0].command, PathBuf::from("/bin/tddy-tools"));
        assert_eq!(
            servers[0].env[0],
            ("TDDY_REMOTE_SESSION_ID".to_string(), "s".to_string())
        );
    }

    #[tokio::test]
    async fn an_unmanaged_launch_keeps_its_own_mcp_servers() {
        let servers = a_launcher()
            .mcp_servers_for(&a_launch("opencode"))
            .await
            .expect("servers");

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
            DropContext {
                uid: 4242,
                gid: 4242,
                privileged: false,
                setpriv: None,
            },
            Arc::new(|_| Ok(None)),
            Arc::new(|_| Err(SubagentError::from("unused"))),
        )
        .with_supervisor_relay(RelayEndpoint {
            spawner: supervisor.clone(),
            socket_dir: dir.path().join("r"),
        });
        (launcher, supervisor, dir)
    }

    #[tokio::test]
    async fn an_unprivileged_daemon_asks_the_supervisor_to_run_another_users_agent() {
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();
        let mut launch = a_launch("opencode");
        launch.session_os_user = "root".to_string();

        let transport = launcher.launch_transport(&launch).await.expect("transport");

        assert!(matches!(transport, AgentTransport::Stream(_)));
        let asked = supervisor.asked.lock().expect("asked");
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].os_user, "root");
        assert_eq!(asked[0].args, ["acp"]);
        assert_eq!(asked[0].working_dir, Some(PathBuf::from("/work/tree")));
    }

    #[tokio::test]
    async fn the_daemons_own_user_needs_no_supervisor() {
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();
        let mut launch = a_launch("opencode");
        launch.session_os_user = this_user();
        let launcher = DaemonCodingAgentLauncher {
            drop: the_daemons_own_identity(),
            ..launcher
        };

        let transport = launcher.launch_transport(&launch).await.expect("transport");

        assert!(matches!(transport, AgentTransport::Command(_)));
        assert!(supervisor.asked.lock().expect("asked").is_empty());
    }

    #[tokio::test]
    async fn a_supervised_launch_still_applies_the_route_it_sends_as_environment() {
        let (launcher, supervisor, _dir) = an_unprivileged_daemon_with_a_supervisor();
        let launcher = DaemonCodingAgentLauncher {
            route: Arc::new(|_| Ok(Some(a_route()))),
            ..launcher
        };
        let mut launch = a_launch("claude-acp");
        launch.session_os_user = "root".to_string();
        launch.def.provider_account = Some("meta".to_string());

        launcher.launch_transport(&launch).await.expect("transport");

        let asked = supervisor.asked.lock().expect("asked");
        assert_eq!(
            asked[0].env.get("API_KEY").map(String::as_str),
            Some("sekret")
        );
    }

    #[tokio::test]
    async fn a_harness_with_no_command_is_refused() {
        let refusal = command(&a_launcher(), &a_launch("claude"))
            .await
            .expect_err("refused");

        assert!(refusal.contains("'claude' harness"), "{refusal}");
    }
}
