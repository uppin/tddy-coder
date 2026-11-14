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

mod github;
mod ops;
mod shape;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tddy_core::changeset::Stack;
use tddy_core::project_stack_store::{ProjectStackStore, StackRecord, StackStoreError};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::WorkflowError;
use tddy_github::pr_api::{GithubPrApi, GithubPrInsightApi, RealGithubPrApi};
use tddy_host_service::GithubCli;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_tool_engine::stack_tools::{
    execute_stack_tool, is_stack_tool_offered_to, stack_caller_role, ParentStack, StackCaller,
    StackCallerFacts, StackOp, StackScope, StackToolPort,
};
use tddy_tool_engine::ToolOutcome;

use super::project_home::caller::record_call;
use super::ExecToolRpcHandler;
use github::GithubSource;
use ops::OpEnv;

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
pub(super) fn stack_caller_of(
    sessions_base: &Path,
    session_id: &str,
) -> Result<Option<StackCaller>, Status> {
    let refused = |what: &str, why: &dyn std::fmt::Display| {
        Status::permission_denied(format!(
            "the stack tools are refused: session {session_id}'s {what} cannot be read ({why})"
        ))
    };
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata =
        tddy_core::read_session_metadata(&session_dir).map_err(|e| refused("record", &e))?;
    let changeset = changeset_of(&session_dir).map_err(|e| refused("changeset", &e))?;
    let parent_stack = match changeset.orchestrator_session_id.as_deref() {
        Some(orchestrator) => node_binding(
            sessions_base,
            &metadata.project_id,
            orchestrator,
            session_id,
        )
        .map_err(|e| refused("orchestrator's stack", &e))?,
        None => None,
    };
    let facts = StackCallerFacts {
        session_role: metadata.session_role.as_deref(),
        own_stack_id: changeset.stack_id.as_deref(),
        parent_stack: parent_stack
            .as_ref()
            .map(|(stack_id, binds_this_session)| ParentStack {
                stack_id,
                binds_this_session: *binds_this_session,
            }),
    };
    Ok(stack_caller_role(&facts).map(|role| StackCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id.clone(),
        role,
    }))
}

/// The session's changeset; a session that has none yet (an ordinary one) has the default.
fn changeset_of(session_dir: &Path) -> Result<tddy_core::changeset::Changeset, WorkflowError> {
    match tddy_core::read_changeset(session_dir) {
        Err(WorkflowError::ChangesetMissing(_)) => Ok(Default::default()),
        read => read,
    }
}

/// The stack `orchestrator`'s changeset names and whether one of its nodes is bound to
/// `session_id`; `None` when that orchestrator is not on this host, names no stack, or names one the
/// project no longer holds.
fn node_binding(
    sessions_base: &Path,
    project_id: &str,
    orchestrator: &str,
    session_id: &str,
) -> Result<Option<(String, bool)>, String> {
    let orchestrator_dir = unified_session_dir_path(sessions_base, orchestrator);
    let changeset = match tddy_core::read_changeset(&orchestrator_dir) {
        Ok(changeset) => changeset,
        Err(WorkflowError::ChangesetMissing(_)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let Some(stack_id) = changeset.stack_id else {
        return Ok(None);
    };
    let store = ProjectStackStore::new(sessions_base.join("projects"));
    let record = match store.read(project_id, &stack_id) {
        Ok(record) => record,
        // A stack the project no longer holds binds no node.
        Err(StackStoreError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let binds_this_session = record
        .stack
        .nodes
        .iter()
        .any(|node| node.session_id.as_deref() == Some(session_id));
    Ok(Some((stack_id, binds_this_session)))
}

impl ExecToolRpcHandler {
    /// Serve `req`'s stack tool: classify the caller, refuse a caller its role does not entitle,
    /// forward a call naming another host, else run it on this host's stacks and record the call.
    pub(super) async fn run_stack_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let tool = &req.tool_name;
        let caller = stack_caller_of(sessions_base, &req.session_id)?.ok_or_else(|| {
            Status::permission_denied(format!(
                "tool {tool} is served only to an orchestrator session, or a session bound to a \
                 node of its orchestrator's stack"
            ))
        })?;
        if !is_stack_tool_offered_to(&caller.role, tool) {
            return Err(Status::permission_denied(format!(
                "tool {tool} is not offered to a session bound to a node of a stack: it may read \
                 its stack's status and its pull requests only"
            )));
        }
        let response = match self.served_on_target_host(req).await? {
            Some(answered) => answered,
            None => {
                let os_user = self.os_user_of(&req.session_token)?;
                let port = self.stack_tool_port(req, os_user, sessions_base);
                response_of(execute_stack_tool(&port, &caller, tool, &req.args_json).await)
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// Serve a stack tool called by a session another host holds — forwarded here by that host,
    /// which holds the stack's owner's side of the call — once the daemon that owns the session has
    /// confirmed it holds an orchestrator role. Such a caller owns no stack here, so a call must name
    /// the stack it acts on.
    pub(super) async fn serve_stack_tool_for_caller_held_elsewhere(
        &self,
        req: &ExecuteToolRequest,
        caller: StackCaller,
        os_user: String,
        sessions_base: &Path,
    ) -> ExecuteToolResponse {
        let port = self.stack_tool_port(req, os_user, sessions_base);
        response_of(execute_stack_tool(&port, &caller, &req.tool_name, &req.args_json).await)
    }

    /// [`DaemonStackTools`] over this host for `req`'s caller, whose sessions are `os_user`'s in
    /// `sessions_base`.
    pub(super) fn stack_tool_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
    ) -> DaemonStackTools {
        DaemonStackTools {
            store: ProjectStackStore::new(sessions_base.join("projects")),
            sessions_base: sessions_base.to_path_buf(),
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

/// The answer a tool's outcome is on the wire.
fn response_of(outcome: ToolOutcome) -> ExecuteToolResponse {
    ExecuteToolResponse {
        result_json: outcome.result_json,
        is_error: outcome.is_error,
        error_message: outcome.error_message,
        job_id: outcome.job_id,
        job_running: outcome.job_running,
    }
}

/// [`StackToolPort`] over this host's stacks, for one caller.
pub(super) struct DaemonStackTools {
    /// This host's stacks, `<sessions base>/projects/<project_id>/stacks/<stack_id>.yaml`.
    store: ProjectStackStore,
    /// The caller's sessions, where a node's child session is read.
    sessions_base: PathBuf,
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
        self.store.list(project_id).map_err(|e| e.to_string())
    }

    async fn create_stack(
        &self,
        caller: &StackCaller,
        title: Option<&str>,
    ) -> Result<StackRecord, String> {
        // Never the caller's own stack: its changeset is not touched.
        self.store
            .create_titled(
                &caller.project_id,
                &caller.session_id,
                title.map(str::to_string),
                Stack {
                    version: 1,
                    nodes: Vec::new(),
                },
            )
            .map_err(|e| e.to_string())
    }

    async fn run_stack_op(&self, scope: &StackScope, op: StackOp) -> Result<Value, String> {
        let env = OpEnv {
            store: self.store.clone(),
            project_id: scope.project_id.clone(),
            stack_id: scope.stack_id.clone(),
            sessions_base: self.sessions_base.clone(),
        };
        let github = GithubSource {
            projects_dir: self.store.projects_dir().to_path_buf(),
            project_id: scope.project_id.clone(),
            os_user: self.os_user.clone(),
            identity: self.identity.clone(),
            github_cli: self.github_cli.clone(),
            clients: Arc::clone(&self.github),
        };
        // The store's locks, git and GitHub are all blocking.
        tokio::task::spawn_blocking(move || ops::run(&env, &github, op))
            .await
            .map_err(|e| format!("the stack operation failed to finish: {e}"))?
    }
}

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
