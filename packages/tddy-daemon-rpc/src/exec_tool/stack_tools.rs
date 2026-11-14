//! The PR-stack plan and read tools ([`tddy_tool_engine::stack_tools`]) answered on this daemon
//! (`#orchestrators` 7/11).
//!
//! A stack tool is routed here from `run_locally`, ahead of the local runner, as the project-home
//! tools are. The caller is classified from its own records ([`stack_caller_of`]), by role alone:
//! an orchestrator-role session (its changeset's `stack_id` being its own, default stack), or a
//! session bound to a node of a stack. No recipe is read (developer, 2026-11-14). A
//! session with none of those roles, or calling a tool its role is not offered, is refused as
//! permission denied. A call whose `daemon_instance_id` names another host is forwarded whole to
//! that host, which serves it for its own stacks once the caller's owner confirms the caller, one
//! hop as the project-home tools are (`project_home_remote`).
//!
//! The work is [`DaemonStackTools`]: the host's [`ProjectStackStore`] (`<sessions base>/projects`),
//! `tddy_pr_stack`'s stack operations and PR reads over it, and GitHub reached as the **project's
//! account** — the token the project row's assignment (or its Local gh) resolves, never the calling
//! session's, which a read-only session does not have.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § PR-management tools.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tddy_core::project_stack_store::{ProjectStackStore, StackRecord};
use tddy_github::pr_api::{GithubPrApi, GithubPrInsightApi, RealGithubPrApi};
use tddy_host_service::GithubCli;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_tool_engine::stack_tools::{StackCaller, StackOp, StackScope, StackToolPort};

use super::ExecToolRpcHandler;

/// A GitHub client a stack tool calls through: the lifecycle operations and the reads, for one
/// repository, authenticated as one account.
pub trait StackGithub: GithubPrApi + GithubPrInsightApi {}

impl<T: GithubPrApi + GithubPrInsightApi + ?Sized> StackGithub for T {}

/// How the stack tools reach GitHub: a client for a project's repository, authenticated as the
/// project account's token. The daemon's own is [`RealStackGithubClients`]; a suite attaches
/// another with [`RpcHandlers::attach_stack_github`](crate::RpcHandlers::attach_stack_github).
pub trait StackGithubClients: Send + Sync {
    /// A client for `repo` (`owner/repo`) sending `token` — the project account's.
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn StackGithub>;
}

/// The GitHub REST API itself.
pub struct RealStackGithubClients;

impl StackGithubClients for RealStackGithubClients {
    fn client_for(&self, repo: &str, token: &str) -> Box<dyn StackGithub> {
        Box::new(RealGithubPrApi::with_token(repo, token))
    }
}

/// The stack role of `session_id` on this host, read from its `.session.yaml` role, its changeset's
/// `stack_id` and `orchestrator_session_id`, and —
/// for a session that names an orchestrator — that orchestrator's stack record; `None` for a session
/// offered no stack tool. A session whose records cannot be read is refused: its role cannot be
/// known.
#[allow(dead_code)] // TODO(stack-plan-tools): reached from `run_stack_tool` once implemented.
pub(super) fn stack_caller_of(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<StackCaller>, Status> {
    // TODO(stack-plan-tools): implement — gather `StackCallerFacts` (role, own stack id, the
    // orchestrator's stack and whether a node binds this session) and classify them with
    // `stack_caller_role`.
    let _ = (sessions_base, session_id);
    Ok(None)
}

impl ExecToolRpcHandler {
    /// Serve `req`'s stack tool: classify the caller, refuse a caller its role does not entitle,
    /// forward a call naming another host, else run it on this host's stacks and record the call.
    #[allow(dead_code)] // TODO(stack-plan-tools): routed from `run_locally` once implemented.
    pub(super) async fn run_stack_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        // TODO(stack-plan-tools): implement — `stack_caller_of`, the role refusal
        // (`is_stack_tool_offered_to`), forwarding by `daemon_instance_id`, `execute_stack_tool`
        // over `stack_tool_port`, `record_call`.
        let _ = sessions_base;
        Err(Status::unimplemented(format!(
            "tool {} is not served on this host yet",
            req.tool_name
        )))
    }

    /// [`DaemonStackTools`] over this host for `req`'s caller, whose sessions are `os_user`'s in
    /// `sessions_base`.
    #[allow(dead_code)] // TODO(stack-plan-tools): reached from `run_stack_tool` once implemented.
    pub(super) fn stack_tool_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
    ) -> DaemonStackTools {
        DaemonStackTools {
            store: ProjectStackStore::new(sessions_base.join("projects")),
            sessions_base: sessions_base.to_path_buf(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            os_user,
            daemon_instance_id: tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(
                &self.config,
            ),
            identity: SessionAccountAccess::new(
                self.project_home_host.credential_vaults.clone(),
                self.user_resolver.clone(),
                &req.session_token,
            ),
            github_cli: self.project_home_host.github_cli.clone(),
            github: self
                .project_home_host
                .stack_github
                .get()
                .cloned()
                .unwrap_or_else(|| Arc::new(RealStackGithubClients)),
        }
    }
}

/// [`StackToolPort`] over this host's stacks, for one caller.
#[allow(dead_code)] // TODO(stack-plan-tools): every field is read once the port is implemented.
pub(super) struct DaemonStackTools {
    /// This host's stacks, `<sessions base>/projects/<project_id>/stacks/<stack_id>.yaml`.
    store: ProjectStackStore,
    /// The caller's sessions, where a node's child session and a pr-stack orchestrator's changeset
    /// are read.
    sessions_base: PathBuf,
    /// Where the caller's `projects.yaml` is read: the project's repository and account.
    tddy_data_dir: PathBuf,
    os_user: String,
    /// This host, as the eligible-daemon roster names it.
    daemon_instance_id: String,
    /// The signed-in person's vault, which the project's assigned GitHub account is read from.
    identity: SessionAccountAccess,
    /// The `gh` a Local gh project's token is asked of.
    github_cli: GithubCli,
    github: Arc<dyn StackGithubClients>,
}

#[async_trait]
impl StackToolPort for DaemonStackTools {
    fn daemon_instance_id(&self) -> String {
        self.daemon_instance_id.clone()
    }

    async fn list_stacks(&self, project_id: &str) -> Result<Vec<StackRecord>, String> {
        // TODO(stack-plan-tools): implement — `ProjectStackStore::list`.
        let _ = project_id;
        Err(NOT_YET.to_string())
    }

    async fn create_stack(
        &self,
        caller: &StackCaller,
        title: Option<&str>,
    ) -> Result<StackRecord, String> {
        // TODO(stack-plan-tools): implement — `ProjectStackStore::create_titled` with an empty
        // stack. It never becomes the caller's own stack.
        let _ = (caller, title);
        Err(NOT_YET.to_string())
    }

    async fn run_stack_op(&self, scope: &StackScope, op: StackOp) -> Result<Value, String> {
        // TODO(stack-plan-tools): implement — the store's read/update with `tddy_pr_stack`'s
        // stack operations, `pr_insight` for the reads, the project's repository and its account's
        // token for GitHub, and the answers `tddy-tools` shaped before.
        let _ = (scope, op);
        Err(NOT_YET.to_string())
    }
}

const NOT_YET: &str = "the stack tools are not served on this host yet";

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::stack_fixtures::{a_stack_host, stack_id_of, A_PROJECT};
    use tddy_core::changeset::{Changeset, Stack, StackNode};
    use tddy_tool_engine::stack_tools::StackCallerRole;

    use super::*;

    fn a_stack_planning(child: Option<&str>) -> Stack {
        Stack {
            version: 1,
            nodes: vec![StackNode {
                node_id: "n1".to_string(),
                title: "Store".to_string(),
                session_id: child.map(str::to_string),
                ..StackNode::default()
            }],
        }
    }

    fn with_role(dir: &Path, role: &str) {
        let mut meta = tddy_core::read_session_metadata(dir).unwrap();
        meta.session_role = Some(role.to_string());
        tddy_core::session_metadata::write_session_metadata(dir, &meta).unwrap();
    }

    #[test]
    fn an_orchestrator_role_session_is_classified_an_orchestrator_of_its_project() {
        // Given main's session of a project
        let host = a_stack_host();
        let dir = host.session("main", Changeset::default());
        with_role(&dir, "project-home");

        // When it is classified
        let caller = stack_caller_of(host.base(), "main").unwrap();

        // Then it is an orchestrator of its project
        assert_eq!(
            caller,
            Some(StackCaller {
                session_id: "main".to_string(),
                project_id: A_PROJECT.to_string(),
                role: StackCallerRole::Orchestrator { own_stack_id: None },
            })
        );
    }

    #[test]
    fn an_orchestrator_is_classified_owning_the_stack_its_changeset_names() {
        // Given an orchestrator-role session whose stack the project's store holds
        let host = a_stack_host();
        let dir = host.orchestrator("orch", a_stack_planning(None));
        with_role(&dir, "orchestrator");
        let own = stack_id_of(&dir).expect("the changeset names its stack");

        // When it is classified
        let caller = stack_caller_of(host.base(), "orch").unwrap();

        // Then it is an orchestrator owning that stack
        assert_eq!(
            caller.map(|c| c.role),
            Some(StackCallerRole::Orchestrator {
                own_stack_id: Some(own)
            })
        );
    }

    #[test]
    fn a_pr_stack_session_with_no_orchestrator_role_is_classified_with_no_stack_role() {
        // Given a pr-stack recipe session (no role) whose changeset names its stack
        let host = a_stack_host();
        host.orchestrator("orch", a_stack_planning(None));

        // When it is classified
        let caller = stack_caller_of(host.base(), "orch").unwrap();

        // Then the recipe and the stack give it no stack role
        assert_eq!(caller, None);
    }

    #[test]
    fn a_session_its_orchestrators_stack_binds_is_classified_a_node_of_that_stack() {
        // Given an orchestrator whose node n1 is bound to a child session naming it
        let host = a_stack_host();
        let orch = host.orchestrator("orch", a_stack_planning(Some("child")));
        with_role(&orch, "orchestrator");
        let stack_id = stack_id_of(&orch).unwrap();
        host.session(
            "child",
            Changeset {
                orchestrator_session_id: Some("orch".to_string()),
                ..Changeset::default()
            },
        );

        // When the child is classified
        let caller = stack_caller_of(host.base(), "child").unwrap();

        // Then it is a node of that stack
        assert_eq!(
            caller.map(|c| c.role),
            Some(StackCallerRole::StackNode { stack_id })
        );
    }

    #[test]
    fn a_session_with_no_record_is_refused_rather_than_classified_ordinary() {
        // Given a session id with nothing on disk
        let host = a_stack_host();

        // When it is classified
        let refusal = stack_caller_of(host.base(), "ghost").expect_err("its role cannot be known");

        // Then it is refused as permission denied, naming the session
        assert_eq!(refusal.code(), tddy_rpc::Code::PermissionDenied);
        assert!(refusal.message().contains("ghost"), "{}", refusal.message());
    }
}
