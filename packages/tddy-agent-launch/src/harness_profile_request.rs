//! A start that names a harness profile as its `agent`, turned into the start the profile stands
//! for: its harness as the session type or tool-session agent, its model when the request chose
//! none, and its account as the `agent_definition` level of the route.
//!
//! Resolved once, at the top of the start, so every session type's own route resolution reads an
//! ordinary request.

use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::provider_route::{choose_route, RouteSources};
use tddy_core::session_metadata::{is_orchestrator_role, ORCHESTRATOR_ROLE};
use tddy_discovery::harness_profile::{load_harness_profiles, HarnessProfile};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::CancellationToken;

use crate::harness_profile_start::profile_start;
use crate::launch_ports::LaunchSessions;
use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{session_route_sources, status_of_refusal};

impl LaunchSessions {
    /// Start the session `req` asks for — through the harness profile its `agent` names, if any:
    /// a profile is not a def, it stands for a session type or agent on an account.
    ///
    /// This is the start every caller-facing path takes (`StartSession`, `StreamStartSession`, a
    /// split's workspace start, a project-home `StartSession`), so a request naming a
    /// `session_role` is admitted here ([`admit_a_requested_role`]): only `orchestrator` may be
    /// asked for, by a person or by an orchestrator's agent, and is then placed on the project's
    /// home worktree. `project-home`, main's role, is the daemon's to grant: only
    /// [`Self::start_session_with_role`] starts a session with it.
    pub async fn start_session_core(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_session_core_cancellable(req, progress, &CancellationToken::new())
            .await
    }

    /// [`Self::start_session_core`] for a start that can be cancelled: once `cancel` is signalled the
    /// clone is stopped, no further step begins and the start ends `CANCELLED`.
    pub async fn start_session_core_cancellable(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
        cancel: &CancellationToken,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.admit_the_requested_role(&req)?;
        let req = self.apply_harness_profile(req)?;
        let req = self.place_an_orchestrator(req)?;
        // Refused before routing, on the session type the profile resolved: a start this daemon
        // would refuse is not forwarded to a peer to be refused there, and nothing is created for it.
        crate::start_request_checks::refuse_a_conflicting_tmux_run_start(
            &req,
            Some(&*self.coding_agents),
        )?;
        self.start_session_with_role(req, progress, cancel).await
    }

    /// Start `req`, its harness profile already applied, with the standing role it carries.
    /// Reached only from inside the daemon — `EnsureProjectHomeSession` is the one caller, and it
    /// applies the profile itself so it can check the session type the profile resolves to —
    /// never from a request a client sent.
    pub(crate) async fn start_session_with_role(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
        cancel: &CancellationToken,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.start_session_on_request(req, progress, cancel).await
    }

    /// `req` as the profile its `agent` names in `<tddyhome>/agents/` would start it; `req`
    /// unchanged when its `agent` names no profile.
    pub(crate) fn apply_harness_profile(
        &self,
        req: StartSessionRequest,
    ) -> Result<StartSessionRequest, Status> {
        let agents_dir = self.tddy_data_dir.join("agents");
        let name = req.agent.trim();
        let Some(profile) = load_harness_profiles(&agents_dir)
            .into_iter()
            .find(|profile| profile.name == name)
        else {
            return Ok(req);
        };
        let access = self.host.session_account_access(&req.session_token);
        request_on_profile(req, &profile, &access, self.coding_agents.agents())
    }
}

/// The start `profile` stands for, with the request's own session-level account choice and model
/// kept. The chosen account is resolved in the starter's vault (by id or label) and carried by its
/// id; one the vault does not hold refuses the start.
pub fn request_on_profile(
    req: StartSessionRequest,
    profile: &HarnessProfile,
    access: &SessionAccountAccess,
    agents: &CodingAgentRegistry,
) -> Result<StartSessionRequest, Status> {
    let start = profile_start(profile, agents).map_err(Status::invalid_argument)?;
    let sources = RouteSources {
        agent_definition: Some(start.choice),
        ..session_route_sources(&req)
    };
    let provider_account_id = match choose_route(&sources) {
        Some(chosen) => access
            .provider_account(&chosen)
            .map_err(|refusal| profile_refusal(profile, &chosen, status_of_refusal(refusal)))?
            .account
            .as_str()
            .to_string(),
        None => String::new(),
    };
    let model = if req.model.trim().is_empty() {
        start.model
    } else {
        req.model.clone()
    };
    Ok(StartSessionRequest {
        session_type: start.session_type.unwrap_or_default(),
        agent: start.agent.unwrap_or_default(),
        model,
        provider_account_id,
        ..req
    })
}

/// A caller-sent start may not name a standing role; see [`LaunchSessions::start_session_core`].
pub(crate) fn refuse_a_requested_role(req: &StartSessionRequest) -> Result<(), Status> {
    if req.session_role.trim().is_empty() {
        return Ok(());
    }
    Err(Status::permission_denied(format!(
        "a session's role is granted by the daemon, not requested: StartSession may not set \
         session_role (got {:?})",
        req.session_role
    )))
}

/// Who asks a start for a standing role (`#orchestrators` 1/11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoleRequester<'a> {
    /// A person: a web or CLI caller presenting their own token.
    Person,
    /// A session's agent (its token carries `TokenPrincipal::Agent`), with the role its own
    /// session holds — `None` for an ordinary session.
    Agent { own_role: Option<&'a str> },
}

/// Admit the standing role `req` asks for, by who asks: an ordinary start (no role) from anyone;
/// `orchestrator` from a person, or from an agent whose own session holds an orchestrator role
/// (`tddy_core::session_metadata::is_orchestrator_role`). Every other agent asking for it, and
/// anyone asking for `project-home` — main's role, the daemon's alone to grant — is refused
/// `PERMISSION_DENIED`, since an orchestrator role reaches the project's other sessions.
pub(crate) fn admit_a_requested_role(
    requester: RoleRequester<'_>,
    req: &StartSessionRequest,
) -> Result<(), Status> {
    if req.session_role.trim() != ORCHESTRATOR_ROLE {
        return refuse_a_requested_role(req);
    }
    match requester {
        RoleRequester::Person => Ok(()),
        RoleRequester::Agent { own_role } if own_role.is_some_and(is_orchestrator_role) => Ok(()),
        RoleRequester::Agent { .. } => Err(Status::permission_denied(format!(
            "only a person or an orchestrator may start a session in the {ORCHESTRATOR_ROLE} role: \
             it reaches the project's other sessions"
        ))),
    }
}

/// `status`, with the profile and the account it was asked for named in its message.
fn profile_refusal(profile: &HarnessProfile, account: &str, status: Status) -> Status {
    Status {
        code: status.code,
        message: format!(
            "harness profile {:?} runs on provider account {account:?}: {}",
            profile.name, status.message
        ),
    }
}

#[cfg(test)]
mod role_admission_tests {
    use super::*;
    use tddy_core::session_metadata::{ORCHESTRATOR_ROLE, PROJECT_HOME_ROLE};
    use tddy_rpc::Code;

    fn a_start_asking_for(role: &str) -> StartSessionRequest {
        StartSessionRequest {
            session_type: "claude-cli".to_string(),
            session_role: role.to_string(),
            ..Default::default()
        }
    }

    fn an_agent_of(own_role: Option<&str>) -> RoleRequester<'_> {
        RoleRequester::Agent { own_role }
    }

    /// The code each start is answered with: `None` admitted, `Some(code)` refused.
    fn answered(requester: RoleRequester<'_>, role: &str) -> Option<Code> {
        admit_a_requested_role(requester, &a_start_asking_for(role))
            .err()
            .map(|status| status.code)
    }

    #[test]
    fn a_person_may_ask_for_the_orchestrator_role() {
        assert_eq!(answered(RoleRequester::Person, ORCHESTRATOR_ROLE), None);
    }

    #[test]
    fn mains_agent_and_an_orchestrators_agent_may_ask_for_it() {
        // When
        let answers = [PROJECT_HOME_ROLE, ORCHESTRATOR_ROLE]
            .map(|own| answered(an_agent_of(Some(own)), ORCHESTRATOR_ROLE));

        // Then
        assert_eq!(answers, [None, None]);
    }

    #[test]
    fn an_agent_holding_no_orchestrator_role_is_refused_it() {
        // When
        let answers =
            [None, Some("reviewer")].map(|own| answered(an_agent_of(own), ORCHESTRATOR_ROLE));

        // Then
        assert_eq!(
            answers,
            [Some(Code::PermissionDenied), Some(Code::PermissionDenied)]
        );
    }

    #[test]
    fn nobody_may_ask_for_project_home() {
        // When
        let answers = [
            RoleRequester::Person,
            an_agent_of(Some(PROJECT_HOME_ROLE)),
            an_agent_of(Some(ORCHESTRATOR_ROLE)),
        ]
        .map(|requester| answered(requester, PROJECT_HOME_ROLE));

        // Then
        assert_eq!(answers, [Some(Code::PermissionDenied); 3]);
    }

    #[test]
    fn a_role_nobody_defines_is_refused_naming_it() {
        // When
        let refusal =
            admit_a_requested_role(RoleRequester::Person, &a_start_asking_for("reviewer"))
                .expect_err("an unknown role is refused");

        // Then
        assert_eq!(refusal.code, Code::PermissionDenied);
        assert!(refusal.message.contains("reviewer"), "{}", refusal.message);
    }

    #[test]
    fn an_ordinary_start_is_admitted_from_anyone() {
        // When
        let answers =
            [RoleRequester::Person, an_agent_of(None)].map(|requester| answered(requester, ""));

        // Then
        assert_eq!(answers, [None, None]);
    }
}
