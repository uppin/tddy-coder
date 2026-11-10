//! A headless launch planned from the agent's spec: its command, its conversation's files and session
//! pointer, its approvals, and the sweep of what a stopped daemon left.

use crate::coding_agent_as_user::run_as_session_user;
use crate::coding_agent_launcher::DaemonCodingAgentLauncher;
use crate::coding_agent_launcher::HeadlessWiring;
use crate::coding_agent_launcher::APPROVAL_PROMPT_TOOL;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::headless::SpecHeadlessRenderer;
use tddy_coding_agent_specs::slots::headless_mcp::mcp_written_paths;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_coding_agent_specs::McpSlot;
use tddy_coding_subagent::launcher::{record_written_files, SESSION_TOKEN_ENV};
use tddy_coding_subagent::refuse_unless_registry_confinable;
use tddy_coding_subagent::CodingAgentLaunch;
use tddy_core::adapters::AdapterRegistry;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::AcpAgentCommand;
use tddy_core::HeadlessCommand;
use tddy_core::McpServerSpec;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_base_instance_id_for_config;
use tddy_discovery::subagent::SubagentError;
use tddy_subagent_worktree::ConversationId;

/// Whether `spec`'s headless MCP slot (its agent's own when the headless block names none) registers
/// the tool server inline, on the argv.
fn registers_mcp_inline(spec: &CodingAgentSpec) -> bool {
    matches!(
        spec.headless()
            .and_then(|headless| headless.mcp.as_ref())
            .unwrap_or(&spec.agent.mcp),
        McpSlot::Inline { .. }
    )
}

impl DaemonCodingAgentLauncher {
    /// The headless command of a conversation of any headless harness, from its declaration
    /// (`claude.yaml`, `cursor.yaml`, `codex.yaml`, `oz.yaml`): `agents.resolve`
    /// names the binary (refused when the session runs as another OS user, as by hand), the
    /// conversation directory is claimed as by hand and becomes the command's `scratch_dir`, the
    /// tool server and the approval tool become its `mcp_server` and `permission_prompt_tool`, the
    /// route's env its `env`, and a `SpecHeadlessRenderer` its `renderer`. Approvals are bound once
    /// the plan has written everything, as by hand. What the MCP slot will write is recorded in the
    /// claimed directory first ([`Self::record_written_files`]).
    pub(super) fn headless_plan_from_spec(
        &self,
        launch: &CodingAgentLaunch,
        wiring: &HeadlessWiring,
        conversation: &str,
        agents: &HostCodingAgents,
    ) -> Result<HeadlessCommand, SubagentError> {
        let harness = launch.def.harness.as_str();
        let agent = agents
            .resolve(harness, None)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        let program = agent.tool.binary.clone();
        self.refuse_unless_daemon_user(launch, &program.to_string_lossy())?;
        let env = self.headless_provider_env(launch)?;
        let mut server = self.tool_server_spec(launch)?;
        let files = self.claim_conversation_files(launch, wiring, conversation)?;
        let scratch_dir = files.dir.clone();
        self.record_written_files(launch, &agent.spec, &scratch_dir)?;
        // What the spec declares, not which agent it is: an agent that routes its permission prompts
        // to the tool server says so (`headless.approval_tool`), and one whose MCP slot is inline
        // registers the server on its argv, where a credential must not be: the server's variables
        // go into the conversation's session pointer instead, which the server finds from its cwd.
        let routes_approvals = agent
            .spec
            .headless()
            .is_some_and(|headless| headless.approval_tool);
        let registers_inline = registers_mcp_inline(&agent.spec);
        let renderer = SpecHeadlessRenderer::new(
            agent.spec.clone(),
            agent.tool.clone(),
            &agent.deps,
            AdapterRegistry::builtin(),
        )
        .map_err(|reason| Self::refuse(launch, &reason.to_string()))?
        .with_env_base(agent.env_base.clone());
        if routes_approvals {
            self.bind_approvals(launch, conversation, &server)?;
        }
        if registers_inline {
            self.write_conversation_pointer(launch, &scratch_dir, std::mem::take(&mut server.env))?;
            server.cwd = Some(scratch_dir.clone());
        }
        files.keep();
        Ok(HeadlessCommand {
            env,
            cwd: Some(launch.worktree.clone()),
            model: Some(launch.def.model.clone()),
            permission_prompt_tool: routes_approvals.then(|| APPROVAL_PROMPT_TOOL.to_string()),
            disallowed_tools: Vec::new(),
            profile: None,
            mcp_server: Some(server),
            renderer: Some(Arc::new(renderer)),
            scratch_dir: Some(scratch_dir),
            // The driver's own deadline: no conversation turn waits unbounded.
            turn_deadline: None,
        })
    }

    /// Writes the conversation's session pointer, `<scratch_dir>/.tddy/session.json` (owner-only),
    /// carrying `server_env` — the conversation id and the minted credential — in its `remote`
    /// map. It lives with the conversation directory, and is removed with it when the conversation
    /// closes. The pointer's owner is this daemon.
    fn write_conversation_pointer(
        &self,
        launch: &CodingAgentLaunch,
        scratch_dir: &Path,
        server_env: Vec<(String, String)>,
    ) -> Result<(), SubagentError> {
        use tddy_core::session_pointer::{
            SessionPointer, SessionPointerOwner, SESSION_POINTER_FILE,
        };

        let pointer = SessionPointer {
            socket: None,
            repo_dir: launch.worktree.clone(),
            session_dir: None,
            remote: server_env.into_iter().collect(),
            notify: std::collections::BTreeMap::new(),
            owner: Some(SessionPointerOwner::this_process()),
        };
        let path = scratch_dir.join(SESSION_POINTER_FILE);
        serde_json::to_vec_pretty(&pointer)
            .map_err(|e| e.to_string())
            .and_then(|contents| {
                tddy_core::atomic_file::write_atomic_with_mode(&path, contents, 0o600)
                    .map_err(|e| e.to_string())
            })
            .map_err(|reason| {
                Self::refuse(
                    launch,
                    &format!(
                        "its conversation's session pointer could not be written at {}: {reason}",
                        path.display()
                    ),
                )
            })
    }

    /// The environment a headless CLI takes from the session's provider route, when it runs on one.
    fn headless_provider_env(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<(String, String)>, SubagentError> {
        match launch.def.provider_account.as_deref() {
            Some(account) => {
                let route = (self.route)(launch)?.ok_or_else(|| {
                    Self::refuse(
                        launch,
                        &format!("provider account '{account}' resolved to no route"),
                    )
                })?;
                headless_route_env(launch, &route)
            }
            None => Ok(Vec::new()),
        }
    }

    /// The command a headless conversation of `launch` runs its CLI with. Refused rather than started
    /// some weaker way: a launch the daemon cannot honour exactly is named, not run.
    pub(crate) fn headless_plan(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<HeadlessCommand, SubagentError> {
        let wiring = self.headless.as_ref().ok_or_else(|| {
            Self::refuse(launch, "this daemon has no headless coding-agent wiring")
        })?;
        refuse_unless_registry_confinable(
            self.coding_agents.agents(),
            &launch.def.harness,
            launch.managed,
        )
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
