//! A start that names a harness profile as its `agent`, turned into the start the profile stands
//! for: its harness as the session type or tool-session agent, its model when the request chose
//! none, and its account as the `agent_definition` level of the route.
//!
//! Resolved once, at the top of the start, so every session type's own route resolution reads an
//! ordinary request.

use tddy_core::provider_route::{choose_route, RouteSources};
use tddy_discovery::harness_profile::{load_harness_profiles, HarnessProfile};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;

use crate::harness_profile_start::profile_start;
use crate::launch_ports::LaunchSessions;
use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{session_route_sources, status_of_refusal};

impl LaunchSessions {
    /// Start the session `req` asks for — through the harness profile its `agent` names, if any:
    /// a profile is not a def, it stands for a session type or agent on an account.
    ///
    /// This is the start every caller-facing path takes (`StartSession`, `StreamStartSession`, a
    /// split's workspace start, a project-home `StartProjectSession`), so a request naming a
    /// `session_role` is refused here: a role is granted by the daemon, never asked for, since a
    /// project-home role reaches the project's other sessions. Only
    /// [`Self::start_session_with_role`] starts a session with one.
    pub async fn start_session_core(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        refuse_a_requested_role(&req)?;
        self.start_session_with_role(req, progress).await
    }

    /// Start `req` with the standing role it carries. Reached only from inside the daemon —
    /// `EnsureProjectHomeSession` is the one caller — never from a request a client sent.
    pub(crate) async fn start_session_with_role(
        &self,
        req: StartSessionRequest,
        progress: &AttachmentProgressSink,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let req = self.apply_harness_profile(req)?;
        self.start_session_on_request(req, progress).await
    }

    /// `req` as the profile its `agent` names in `<tddyhome>/agents/` would start it; `req`
    /// unchanged when its `agent` names no profile.
    fn apply_harness_profile(
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
        request_on_profile(req, &profile, &access)
    }
}

/// The start `profile` stands for, with the request's own session-level account choice and model
/// kept. The chosen account is resolved in the starter's vault (by id or label) and carried by its
/// id; one the vault does not hold refuses the start.
pub fn request_on_profile(
    req: StartSessionRequest,
    profile: &HarnessProfile,
    access: &SessionAccountAccess,
) -> Result<StartSessionRequest, Status> {
    let start = profile_start(profile).map_err(Status::invalid_argument)?;
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
fn refuse_a_requested_role(req: &StartSessionRequest) -> Result<(), Status> {
    if req.session_role.trim().is_empty() {
        return Ok(());
    }
    Err(Status::permission_denied(format!(
        "a session's role is granted by the daemon, not requested: StartSession may not set \
         session_role (got {:?})",
        req.session_role
    )))
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
