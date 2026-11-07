use crate::coding_agent_launcher::headless_route_env;
use crate::coding_agent_launcher::DaemonCodingAgentLauncher;
use crate::coding_agent_launcher::HeadlessWiring;
use crate::coding_agent_launcher::APPROVAL_PROMPT_TOOL;
use std::path::Path;
use std::sync::Arc;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::headless::SpecHeadlessRenderer;
use tddy_coding_agent_specs::McpSlot;
use tddy_coding_subagent::CodingAgentLaunch;
use tddy_core::adapters::AdapterRegistry;
use tddy_core::HeadlessCommand;
use tddy_discovery::subagent::SubagentError;

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
        let registers_inline = matches!(
            agent
                .spec
                .headless()
                .and_then(|headless| headless.mcp.as_ref())
                .unwrap_or(&agent.spec.agent.mcp),
            McpSlot::Inline { .. }
        );
        let renderer = SpecHeadlessRenderer::new(
            agent.spec,
            agent.tool,
            &agent.deps,
            AdapterRegistry::builtin(),
        )
        .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
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
}
