//! The room-admission handshake (PRD § "What attach does" step 3).
//!
//! The facilitating daemon (A) is the authority on who may join `session-{session_id}`. An owning
//! daemon (B) that has never held the project — and so is not yet in the session room — is admitted
//! by A as part of `provision_agent_clone`: A records B in the per-session admission registry,
//! mints a scoped, short-TTL LiveKit token for `daemon-{B}` in `session-{session_id}`, and
//! forwards that token to B along with the StartSession. B joins with that token and nothing else,
//! and re-asks A for a fresh one before it expires — the re-admit loop. When the last agent B owns
//! in that session detaches, A revokes B's admission (PRD § "What detach does"); B's next re-admit
//! returns `FAILED_PRECONDITION`, and B leaves the room. The short TTL is the revocation's teeth:
//! a revoked daemon's current token dies on its own, and no fresh one is issued.
//!
//! `AdmitOwningDaemon` is therefore a **refresh** RPC, not the first admit. The first admit is A's
//! own act (record + mint + forward); the RPC only re-mints for a daemon the registry already
//! holds. A refresh for a daemon the registry does NOT hold is a post-revoke re-admit, and is
//! refused — that is what keeps a revoked daemon out of the room.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_livekit::TokenGenerator;
use tddy_rpc::Status;
use tddy_service::proto::session_admission::{
    SessionAdmissionService, AdmitOwningDaemonRequest, AdmitOwningDaemonResponse,
};

use crate::config::DaemonConfig;
use crate::livekit_peer_discovery::{daemon_rpc_identity, livekit_common_room_connect_strings};
use crate::session_room::session_room_name;

/// The TTL of an admission token. Short on purpose: it is the re-admit cadence and the revocation
/// window. A daemon whose admission has been revoked keeps its current token only until this
/// elapses, and no re-admit is granted afterwards.
pub const ADMISSION_TOKEN_TTL: Duration = Duration::from_secs(300);

/// The margin before token expiry at which an owning daemon re-asks for admission. Keeps the
/// re-admit ahead of the wire, so a room connection never drops between expiry and rejoin.
pub const ADMISSION_RENEW_MARGIN: Duration = Duration::from_secs(60);

/// The set of owning daemons admitted to each session room.
///
/// One registry is shared between the admission RPC (which refreshes entries), the attach path
/// (which records the first admit), the detach path (which revokes an owning daemon when its last
/// agent in a session goes), and the session-delete path (which revokes every owning daemon a
/// session admitted at once). Presence is the only state — the token itself is never stored, only
/// re-minted on each (re-)admit.
#[derive(Default)]
pub struct SessionAdmissionRegistry {
    /// `session_id → owning_daemon_instance_id` entries currently admitted. An entry means "this
    /// daemon currently owns at least one agent in this session, so it may join and stay."
    admitted: Mutex<HashMap<String, HashSet<String>>>,
}

impl SessionAdmissionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or refresh) an admission — the attach path's first admit. Idempotent for the same
    /// `(session, owning)` pair. Returns `true` if this was a new entry.
    pub fn admit(&self, session_id: &str, owning_daemon_instance_id: &str) -> bool {
        let mut g = self.admitted.lock().expect("admission registry poisoned");
        g.entry(session_id.to_string())
            .or_default()
            .insert(owning_daemon_instance_id.to_string())
    }

    /// Revoke a single owning daemon's admission to a session — the last-detach path. Returns
    /// `true` if the entry was present (so the caller can tell a real revocation from a no-op).
    pub fn revoke(&self, session_id: &str, owning_daemon_instance_id: &str) -> bool {
        let mut g = self.admitted.lock().expect("admission registry poisoned");
        let Some(set) = g.get_mut(session_id) else {
            return false;
        };
        let removed = set.remove(owning_daemon_instance_id);
        if set.is_empty() {
            g.remove(session_id);
        }
        removed
    }

    /// Whether an owning daemon is currently admitted — the RPC's gate: a refresh for a daemon the
    /// registry does not hold is a post-revoke re-admit and is refused.
    pub fn is_admitted(&self, session_id: &str, owning_daemon_instance_id: &str) -> bool {
        let g = self.admitted.lock().expect("admission registry poisoned");
        g.get(session_id)
            .map(|set| set.contains(owning_daemon_instance_id))
            .unwrap_or(false)
    }

    /// Revoke every owning daemon admitted to a session — the session-delete path. Returns the
    /// number of daemons revoked, so the delete path can log it (an operator watching the daemon
    /// log can confirm the sweep matched the roster's host count).
    pub fn revoke_all_for_session(&self, session_id: &str) -> usize {
        let mut g = self.admitted.lock().expect("admission registry poisoned");
        g.remove(session_id)
            .map(|set| set.len())
            .unwrap_or(0)
    }
}

/// A closure the admission service consults to refuse admission to a session this daemon does not
/// hold. The facilitating daemon is the authority on its own sessions; an unknown id is `NOT_FOUND`.
pub type SessionExistsChecker = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// `SessionAdmissionService` implementation: re-mints a scoped short-TTL token for an owning daemon
/// the registry already holds. Served on the facilitating daemon's common-room `daemon-{A}`
/// participant, so an owning daemon that is in the session room (but whose token is expiring) can
/// still reach A over the common room it never left.
pub struct SessionAdmissionServiceImpl {
    user_resolver: crate::remote_git_service::UserResolver,
    config: Arc<DaemonConfig>,
    admissions: Arc<SessionAdmissionRegistry>,
    session_exists: SessionExistsChecker,
}

impl SessionAdmissionServiceImpl {
    pub fn new(
        user_resolver: crate::remote_git_service::UserResolver,
        config: Arc<DaemonConfig>,
        admissions: Arc<SessionAdmissionRegistry>,
        session_exists: SessionExistsChecker,
    ) -> Self {
        Self {
            user_resolver,
            config,
            admissions,
            session_exists,
        }
    }

    /// The shared admission registry — so the attach/detach paths can record and revoke against the
    /// same set the RPC refreshes from.
    pub fn admissions(&self) -> Arc<SessionAdmissionRegistry> {
        Arc::clone(&self.admissions)
    }
}

#[async_trait::async_trait]
impl SessionAdmissionService for SessionAdmissionServiceImpl {
    async fn admit_owning_daemon(
        &self,
        request: tddy_rpc::Request<AdmitOwningDaemonRequest>,
    ) -> Result<tddy_rpc::Response<AdmitOwningDaemonResponse>, Status> {
        let req = request.into_inner();
        log::info!(
            "SessionAdmissionService.AdmitOwningDaemon: daemon {} asks to re-admit to session {}",
            req.owning_daemon_instance_id,
            req.session_id
        );

        // The caller's session token is a daemon access token, verified exactly as on every other
        // token-gated RPC. A refresh-kind or expired token is rejected — admission is an RPC
        // credential, not a session-lifetime grant.
        let _github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session token"))?;

        let session_id = req.session_id.trim();
        let owning = req.owning_daemon_instance_id.trim();
        if session_id.is_empty() || owning.is_empty() {
            return Err(Status::invalid_argument(
                "admit_owning_daemon must name both the session being admitted to and the owning \
                 daemon being admitted; half a request names a room with nobody to address",
            ));
        }

        // The facilitating daemon is the authority on this session. An unknown id is `NOT_FOUND`,
        // never a token for a room nobody controls.
        if !(self.session_exists)(session_id) {
            log::warn!(
                "SessionAdmissionService.AdmitOwningDaemon: refusing daemon {owning} — session \
                 {session_id} is not held by this daemon (NOT_FOUND)"
            );
            return Err(Status::not_found(format!(
                "session '{session_id}' is not held by this daemon; it cannot admit an owning \
                 daemon to a room it does not control"
            )));
        }

        // The refresh gate. A daemon the registry does not hold is either a bug (the owning daemon
        // is calling before the facilitating daemon recorded the first admit) or a post-revoke
        // re-admit (its last agent detached). Both are refused the same way: the first admit is the
        // facilitating daemon's own act, never this RPC.
        if !self.admissions.is_admitted(session_id, owning) {
            log::warn!(
                "SessionAdmissionService.AdmitOwningDaemon: refusing daemon {owning} — not in \
                 the admission registry for session {session_id} (FAILED_PRECONDITION: revoked or \
                 never attached)"
            );
            return Err(Status::failed_precondition(format!(
                "owning daemon '{owning}' is not currently admitted to session '{session_id}'; \
                 either its last agent detached and its admission was revoked, or it was never \
                 attached. Attach (or re-attach) an agent first"
            )));
        }

        let room = session_room_name(session_id);
        let identity = daemon_rpc_identity(owning);
        // The livekit url/key/secret are deployment-wide; only the room name is per-session.
        let (_common_room, url, api_key, api_secret) = livekit_common_room_connect_strings(&self.config)
            .map_err(|e| Status::failed_precondition(format!("this daemon cannot admit an owning daemon: {e}")))?;
        let token = TokenGenerator::new(
            api_key,
            api_secret,
            room.clone(),
            identity,
            ADMISSION_TOKEN_TTL,
        )
        .generate()
        .map_err(|e| Status::internal(format!("mint an admission token for {room}: {e}")))?;
        log::info!(
            "SessionAdmissionService.AdmitOwningDaemon: re-admitted daemon {owning} to session \
             {session_id} (room {room}, ttl={}s)",
            ADMISSION_TOKEN_TTL.as_secs()
        );

        Ok(tddy_rpc::Response::new(AdmitOwningDaemonResponse {
            token,
            url,
            room,
            ttl_seconds: ADMISSION_TOKEN_TTL.as_secs(),
        }))
    }
}
