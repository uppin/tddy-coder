//! What this daemon hands `tddy-session-files` so that crate can serve
//! `session_files.SessionFilesService`, and the routing it keeps for itself.
//!
//! The thirteen session-file methods are `tddy-session-files`'; what stays here is the seven
//! answers only a daemon has — which OS user a session token belongs to, where this host keeps its
//! data and its staging area, what it caps an attachment at, which instance id it stamps on a
//! staged entry, which checkout a session's agent guidance is read from, and how long a read of
//! that checkout may take.
//!
//! Eight of the thirteen also **route**: a request naming another daemon is served by that daemon,
//! not here. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-files` may reach for — its module header says so
//! — which is why [`PeerRoutedSessionFiles`] wraps the crate's implementation rather than the crate
//! growing a transport. Every decision below is made by a [`DaemonSessionHost`] method rather
//! than re-derived here, so a request that arrives on the wire and one this daemon makes for itself
//! cannot disagree about which host holds a file.

use std::sync::Arc;

use livekit::prelude::Room;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_session_files::service::{SessionContextScope, SessionContextScopes};
use tddy_session_files::{SessionFilesPorts, SessionFilesServiceImpl};

use super::DaemonSessionHost;
use crate::livekit_peer_discovery::local_instance_id_for_config;

/// The coordinate a forward is addressed at on the peer. A forwarded call has to land on the same
/// method of the same service there, which is where the peer declares these eight — so the name is
/// the one `tddy-service` publishes, the same value `tddy-session-files` serves under and
/// `tddy-daemon-livekit`'s forwarders address.
const SESSION_FILES_SERVICE: &str = tddy_service::SESSION_FILES_SERVICE;

/// The label the daemon's exec-tool authorization logs name a context read by.
///
/// The three context RPCs pass their own proto method name there; this port serves all three
/// behind one [`SessionContextScopes::scope_for`], which is not told which. Naming the surface
/// rather than picking one of the three keeps the log honest — the refusals themselves carry no
/// method name in either case.
const CONTEXT_SCOPE_CALLER: &str = "SessionFilesService context read";

/// The common-room handle a forward is sent over.
type CommonRoomSlot = Arc<tokio::sync::RwLock<Option<Arc<Room>>>>;

impl DaemonSessionHost {
    /// The `session_files.SessionFilesService` entry this daemon registers.
    ///
    /// Built from the *same* config, resolvers and staging base this daemon's `StartSession`
    /// materializes attachments from, so a batch staged over this coordinate is the batch a start
    /// addressed to `the pre-unbundle monolithic RPC coordinate` consumes — a second staging base here would mean
    /// an upload was invisible to the session it was staged for.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn session_files_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: SESSION_FILES_SERVICE,
            service: Arc::new(tddy_service::SessionFilesServiceServer::new(
                self.session_files_service(),
            )) as Arc<dyn tddy_rpc::RpcService>,
        }
    }

    /// Every coordinate a **session room** serves.
    ///
    /// A session room is reached by the agents inside it, and what they may ask for is whatever
    /// this daemon declares. `#unbundle` node 6 moved the session-file and terminal families onto
    /// their own services and node 7 the roster, conversation and activity ones, so a room serving
    /// `the pre-unbundle monolithic RPC coordinate` alone would have quietly stopped answering a question it had
    /// always answered — an in-room agent's `ReadHostDocument` would come back "unknown service"
    /// rather than with the document, and an in-jail `StreamSessionAgents` addressed at the new
    /// coordinate would find no roster at all.
    ///
    /// The four families served above this crate (`tddy-daemon-rpc`) — Project, Catalog, ExecTool
    /// and PR-stack — come through the [`DaemonRpcFamilies`](crate::DaemonRpcFamilies) port, for
    /// the same reason: a host never given them refuses with `FAILED_PRECONDITION` rather than open
    /// a room that has quietly lost them.
    pub(crate) fn session_room_roster(
        self: &Arc<Self>,
    ) -> Result<tddy_rpc::MultiRpcService, tddy_rpc::Status> {
        let mut entries = vec![
            self.session_files_entry(),
            self.session_agents_entry(),
            self.activity_entry(),
            self.terminal_session_entry(),
            self.session_lifecycle_entry(),
            self.demo_vm_entry(),
        ];
        entries.extend(self.rpc_families()?.service_entries());
        Ok(tddy_rpc::MultiRpcService::new(entries))
    }

    /// This daemon's session-file surface: the crate's thirteen handlers, with the eight routed
    /// ones answered by the daemon that holds the bytes.
    ///
    /// Public because it *is* the surface — the entry above is this served over a transport, and a
    /// caller inside the daemon (or an acceptance test) that holds the service rather than the
    /// entry must make the same routing decision a request on the wire does. Before `#unbundle`
    /// node 6 these were `DaemonSessionHost`'s own methods and routed for every caller; a
    /// surface that routed only for wire callers would serve a request naming another host out of
    /// this host's own directories.
    #[must_use]
    pub fn session_files_service(
        self: &Arc<Self>,
    ) -> svc_peer_routed_session_files::PeerRoutedSessionFiles {
        svc_peer_routed_session_files::PeerRoutedSessionFiles {
            connection: Arc::clone(self),
            local: SessionFilesServiceImpl::new(self.session_files_ports()),
        }
    }

    /// The seven host answers the thirteen handlers need, each read off this daemon.
    fn session_files_ports(self: &Arc<Self>) -> SessionFilesPorts {
        let for_tokens = Arc::clone(self);
        SessionFilesPorts {
            // `resolve_os_user` already draws the crate's two distinct refusals — an unverifiable
            // token is `UNAUTHENTICATED`, a GitHub user with no `users[]` row is
            // `PERMISSION_DENIED` — from this daemon's own config.
            os_users: Arc::new(move |session_token: &str| {
                for_tokens.resolve_os_user(session_token)
            }),
            tddy_data_dir: self.tddy_data_dir.clone(),
            staging_base_dir: self.staging_base_dir.clone(),
            max_attachment_bytes: self.config.max_attachment_bytes,
            daemon_instance_id: local_instance_id_for_config(&self.config),
            context_scopes: Arc::new(SessionsOfThisDaemon {
                connection: Arc::clone(self),
            }),
            // The same budget this daemon gives its other filesystem work, so a context read and a
            // worktree build on this host cannot disagree about how long it may take — and the
            // budget an operator already tunes rather than a second key to discover.
            context_read_deadline: self.config.spawn_worker_request_timeout(),
        }
    }
}

/// Where a context read is served from: the checkout this daemon recorded for the session, under
/// the allow-list row the session's own persisted `session_type` names.
///
/// Both answers come from [`DaemonSessionHost`] rather than being re-derived here, because both
/// are the gate: `resolve_exec_tool_worktree` authenticates the caller and refuses a session id
/// that is not one path segment, and `context_globs_for_session` serves the session's row rather
/// than the `agent` the request claimed. A second derivation of either is a second answer to "may
/// this caller read this file".
struct SessionsOfThisDaemon {
    connection: Arc<DaemonSessionHost>,
}

impl SessionContextScopes for SessionsOfThisDaemon {
    fn scope_for(
        &self,
        session_token: &str,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<SessionContextScope, Status> {
        self.connection
            .session_context_scope(session_token, session_id, requested_agent)
    }
}

impl DaemonSessionHost {
    /// The checkout and allow-list one context read is served under, resolved once.
    ///
    /// Inherent rather than only behind [`SessionContextScopes`] because this daemon reads its own
    /// context too: a split start fetches the codebase host's guidance, and when that host is this
    /// one the read must be gated by the same two calls a wire caller's is
    /// (`svc_split_context_from_codebase_host`). A second derivation would be a second answer to
    /// "may this caller read this file".
    pub(crate) fn session_context_scope(
        &self,
        session_token: &str,
        session_id: &str,
        requested_agent: &str,
    ) -> Result<SessionContextScope, Status> {
        let (sessions_base, worktree_root) =
            self.resolve_exec_tool_worktree(&ExecuteToolRequest {
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
                tool_name: CONTEXT_SCOPE_CALLER.to_string(),
                args_json: String::new(),
                daemon_instance_id: String::new(),
            })?;
        let globs = self.context_globs_for_session(
            CONTEXT_SCOPE_CALLER,
            &sessions_base,
            session_id,
            requested_agent,
        )?;
        Ok(SessionContextScope {
            worktree_root,
            globs,
        })
    }
}

mod svc_peer_routed_session_files;
pub use svc_peer_routed_session_files::*;
