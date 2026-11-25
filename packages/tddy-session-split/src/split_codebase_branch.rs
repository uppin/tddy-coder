//! A split session's agent half holds no checkout, so it records the branch its codebase half works
//! on: the operator's host then lists the agent half with that branch, without asking the codebase
//! host on every listing.

use std::path::Path;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Status;
use tddy_service::proto::session::{
    ListSessionsRequest, ListSessionsResponse, StartSessionRequest,
};

use crate::split_ports::SplitSessions;

const SESSION_SERVICE: &str = "session.SessionService";

/// The first changeset of a split session's agent half: the branch its codebase half works on, and
/// the orchestrator that started it when one did.
pub(crate) fn split_agent_first_changeset(
    codebase_branch: &str,
    orchestrator_session_id: Option<String>,
    orchestrator_daemon_instance_id: Option<String>,
) -> tddy_core::Changeset {
    tddy_core::Changeset {
        branch: Some(codebase_branch.to_string()),
        orchestrator_session_id,
        orchestrator_daemon_instance_id,
        ..tddy_core::Changeset::default()
    }
}

impl SplitSessions {
    /// The branch `codebase_session_id` works on, as `codebase_instance_id` lists it.
    pub(crate) async fn codebase_branch_of(
        &self,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<String, Status> {
        let request = ListSessionsRequest {
            session_token: session_token.to_string(),
            include_peers: false,
        };
        let listed = match self
            .peer_routing
            .rpc_served_by_peer::<_, ListSessionsResponse>(
                SESSION_SERVICE,
                "ListSessions",
                codebase_instance_id,
                &request,
            )
            .await?
        {
            Some(listed) => listed,
            None => return self.local_codebase_branch(codebase_session_id, session_token),
        };
        listed
            .sessions
            .into_iter()
            .find(|s| s.session_id == codebase_session_id)
            .map(|s| s.branch)
            .filter(|branch| !branch.is_empty())
            .ok_or_else(|| {
                Status::not_found(format!(
                    "codebase session {codebase_session_id} on daemon {codebase_instance_id} lists no branch"
                ))
            })
    }

    /// The branch of a codebase session this daemon itself holds: the same changeset its own
    /// `ListSessions` enrichment reads the branch from.
    fn local_codebase_branch(
        &self,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<String, Status> {
        let github_user = (self.agent_roster.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = sessions_base
            .join(tddy_core::output::SESSIONS_SUBDIR)
            .join(codebase_session_id);
        let changeset = tddy_core::read_changeset(&session_dir).map_err(|e| {
            Status::not_found(format!(
                "codebase session {codebase_session_id} on this daemon records no changeset: {e}"
            ))
        })?;
        changeset.branch.filter(|b| !b.is_empty()).ok_or_else(|| {
            Status::not_found(format!(
                "codebase session {codebase_session_id} on this daemon records no branch"
            ))
        })
    }

    /// Write the agent half's first changeset before its agent is spawned: who may answer its
    /// prompts is read from it, and the branch its codebase half works on is read from it on every
    /// listing. A codebase branch that cannot be read fails the start — the branch is not optional
    /// for a split session, and the caller's `Err` path tears the codebase half down.
    pub(crate) async fn record_first_changeset(
        &self,
        session_dir: &Path,
        codebase_instance_id: &str,
        codebase_session_id: &str,
        req: &StartSessionRequest,
    ) -> Result<(), Status> {
        let branch = self
            .codebase_branch_of(
                codebase_instance_id,
                codebase_session_id,
                &req.session_token,
            )
            .await?;
        let link = crate::service_util::OrchestratorLink::of_request(
            &req.orchestrator_session_id,
            &req.orchestrator_daemon_instance_id,
            &local_instance_id_for_config(&self.config),
        );
        let changeset = split_agent_first_changeset(
            &branch,
            link.as_ref().map(|link| link.session_id.clone()),
            link.and_then(|link| link.daemon_instance_id),
        );
        tddy_core::write_changeset(session_dir, &changeset).map_err(|e| {
            Status::internal(format!(
                "failed to record the split session's first changeset: {e}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_changeset_of_a_split_agent_records_its_codebase_branch() {
        // Given a split session whose codebase half works on `test-split-udoo-4`, started by an
        // orchestrator
        let orchestrator = Some("01a16dc6-ab4c-7dc3-844e-baca4514f86e".to_string());

        // When the agent half's first changeset is built
        let changeset =
            split_agent_first_changeset("test-split-udoo-4", orchestrator.clone(), None);

        // Then it names the branch and the orchestrator
        assert_eq!(
            (changeset.branch, changeset.orchestrator_session_id),
            (Some("test-split-udoo-4".to_string()), orchestrator)
        );
    }

    #[test]
    fn a_split_agent_started_by_no_orchestrator_still_records_its_branch() {
        // Given a split session started from the web, by no orchestrator
        // When the agent half's first changeset is built
        let changeset = split_agent_first_changeset("feature/split", None, None);

        // Then it names the branch and no orchestrator
        assert_eq!(
            (changeset.branch, changeset.orchestrator_session_id),
            (Some("feature/split".to_string()), None)
        );
    }
}
