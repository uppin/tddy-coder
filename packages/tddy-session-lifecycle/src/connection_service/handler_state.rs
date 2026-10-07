//! The host state an RPC-family handler above this crate is built from.
//!
//! `tddy-daemon-rpc` builds each family's handler with `from_host(&DaemonSessionHost)`, holding
//! only the fields that family reads. These hand those fields out: the shared ones (`Arc`s, and
//! components built on them) as clones of the same handle, so a handler talks to the peers, spawn
//! client, common room, registry, token store, idle tracker, task registry and jails the host does
//! rather than to copies of them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_model_registry::ModelRegistryStore;
use tddy_rpc::Status;
use tddy_spawn::spawn_worker::SpawnClient;

use super::agent_host_callbacks::AgentRoster;
use super::launch_ports::LaunchSessions;
use super::split_ports::SplitSessions;
use super::svc_materialize_staged_attachment::AttachmentState;
use super::AttachmentMaterialization;
use super::{DaemonSessionHost, LocalExecTools};
use crate::config::DaemonConfig;
use crate::multi_host::EligibleDaemonSource;
use crate::peer_routing::PeerRouting;
use crate::presenter_observer_task::presenter_observer_spawn::PresenterObserverDeps;
use crate::relay_idle::RpcActivity;
use tddy_service::proto::session::SessionAttachment;

impl DaemonSessionHost {
    /// The daemon's configuration.
    #[must_use]
    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    /// Maps a caller's session token to their GitHub login.
    #[must_use]
    pub fn user_resolver(&self) -> tddy_daemon_kernel::SessionUserResolver {
        Arc::clone(&self.user_resolver)
    }

    /// The daemon's data root — the parent of every user's sessions and projects.
    #[must_use]
    pub fn tddy_data_dir(&self) -> &Path {
        &self.tddy_data_dir
    }

    /// The peer daemons this one may route a request to, and their project rows.
    #[must_use]
    pub fn eligible_daemon_source(&self) -> Arc<dyn EligibleDaemonSource> {
        Arc::clone(self.peer_routing.eligible_daemon_source())
    }

    /// The forked spawn worker, when this daemon runs one.
    #[must_use]
    pub fn spawn_client(&self) -> Option<Arc<SpawnClient>> {
        self.spawn_client.clone()
    }

    /// The common-room LiveKit slot a request is forwarded to a peer through, when configured.
    #[must_use]
    pub fn common_room_livekit_room(&self) -> Option<crate::livekit_peer_discovery::CommonRoom> {
        self.peer_routing.common_room_livekit_room().cloned()
    }

    /// This daemon's model registry — the assistants an agent id may resolve to — when one is wired.
    #[must_use]
    pub fn model_registry(&self) -> Option<Arc<ModelRegistryStore>> {
        self.model_registry.clone()
    }

    /// The credential vaults an operator's GitHub token is read from when a PR status is looked up
    /// on their behalf, when they are wired.
    #[must_use]
    pub fn credential_vaults(&self) -> Option<Arc<tddy_daemon_auth::SessionVaults>> {
        self.credential_vaults.clone()
    }

    /// The idle tracker this host bumps on every RPC, shared rather than copied, so a handler's
    /// calls keep the same relay alive the host's own do.
    #[must_use]
    pub fn rpc_activity(&self) -> RpcActivity {
        self.rpc_activity.clone()
    }

    /// How this host routes an addressed request to a peer — the same roster and common-room slot,
    /// so a handler forwards to the daemons the host sees.
    #[must_use]
    pub fn peer_routing(&self) -> PeerRouting {
        self.peer_routing.clone()
    }

    /// Where this host runs an exec tool: the same task registry, jails and hosted clones, so a
    /// tool a handler runs is one the rest of the daemon can see.
    #[must_use]
    pub fn local_exec_tools(&self) -> LocalExecTools {
        LocalExecTools::new(
            self.task_registry.clone(),
            Arc::clone(&self.workspace_sandboxes),
            Arc::clone(&self.workspace_sandbox_provisioner),
            Arc::clone(&self.jail_relaunch),
            Arc::clone(&self.hosted_agent_clones),
        )
    }

    /// How long to wait for the codebase daemon's answer to a split session's forwarded start (see
    /// [`super::agent_roster::split_forward_deadline`]), under this host's config.
    pub fn split_forward_deadline(&self) -> Duration {
        crate::connection_service::agent_roster::split_forward_deadline(&self.config)
    }

    /// The fields the demo-VM RPCs read, shared with this host (the VM table and idle tracker are
    /// the same handles) so a service built from them acts on the host's own VMs.
    pub(crate) fn demo_vm_service_state(&self) -> super::activity_hub::DemoVmState {
        super::activity_hub::DemoVmState {
            demo_vm_state: Arc::clone(&self.demo_vm_state),
            tddy_data_dir: self.tddy_data_dir.clone(),
            user_resolver: Arc::clone(&self.user_resolver),
            rpc_activity: self.rpc_activity.clone(),
            config: self.config.clone(),
        }
    }

    /// The fields the presenter observer reads, shared with this host (the sink and the bus are
    /// the same handles).
    pub(crate) fn presenter_observer_deps(&self) -> PresenterObserverDeps {
        PresenterObserverDeps {
            tddy_data_dir: self.tddy_data_dir.clone(),
            presenter_event_sink: self.presenter_event_sink.clone(),
            session_notification_bus: self.session_notification_bus.clone(),
        }
    }

    /// Start the presenter observer for a freshly spawned workflow session (see
    /// [`PresenterObserverDeps::maybe_spawn_presenter_observer`]), over this host's sinks.
    pub(crate) fn maybe_spawn_presenter_observer(
        &self,
        os_user: &str,
        session_id: &str,
        grpc_port: u16,
    ) {
        self.presenter_observer_deps()
            .maybe_spawn_presenter_observer(os_user, session_id, grpc_port);
    }

    /// The fields attachment materialization reads, lent to it for the length of one call.
    pub(crate) fn attachment_state(&self) -> AttachmentState<'_> {
        AttachmentState {
            config: &self.config,
            tddy_data_dir: &self.tddy_data_dir,
            staging_base_dir: &self.staging_base_dir,
            peer_routing: &self.peer_routing,
        }
    }

    /// Pre-creates `session_dir` when needed and materializes the request's attachments before
    /// spawn (see [`AttachmentState::prepare_session_attachments`]), over this host's state.
    pub(crate) async fn prepare_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        self.attachment_state()
            .prepare_session_attachments(ctx)
            .await
    }

    /// The same fields, owned, plus this host's callbacks: the handle the agent topic's methods
    /// live on, for the places a borrowed state cannot go (a task, a `'static` closure).
    ///
    /// Built per call. The callbacks are a clone of this host, whose every mutable field is behind
    /// an `Arc`.
    pub(crate) fn agent_roster(&self) -> AgentRoster {
        AgentRoster {
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            user_resolver: self.user_resolver.clone(),
            peer_routing: self.peer_routing.clone(),
            room_roster: Arc::clone(&self.room_roster),
            session_rooms: Arc::clone(&self.session_rooms),
            session_agent_rosters: Arc::clone(&self.session_agent_rosters),
            session_agent_clones: Arc::clone(&self.session_agent_clones),
            hosted_agent_clones: Arc::clone(&self.hosted_agent_clones),
            roster_keepalive_interval: self.roster_keepalive_interval,
            session_admissions: Arc::clone(&self.session_admissions),
            model_registry: self.model_registry.clone(),
            host: Arc::new(self.clone()),
        }
    }

    /// The split topic's fields, owned, plus this host's callbacks: the handle the split topic's
    /// methods live on, for the places a borrowed state cannot go.
    ///
    /// Built per call, like [`Self::agent_roster`]. The callbacks are a clone of this host, whose
    /// every mutable field is behind an `Arc`.
    pub(crate) fn split_sessions(&self) -> SplitSessions {
        SplitSessions {
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            staging_base_dir: self.staging_base_dir.clone(),
            peer_routing: self.peer_routing.clone(),
            session_rooms: Arc::clone(&self.session_rooms),
            workspace_sandboxes: Arc::clone(&self.workspace_sandboxes),
            workspace_sandbox_provisioner: Arc::clone(&self.workspace_sandbox_provisioner),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            session_tokens: self.session_tokens.clone(),
            agent_roster: self.agent_roster(),
            host: Arc::new(self.clone()),
        }
    }

    /// The launch topic's fields, owned, plus this host's callbacks: the handle the launch topic's
    /// methods live on, for the places a borrowed state cannot go.
    ///
    /// Built per call, like [`Self::agent_roster`]. The callbacks are a clone of this host, whose
    /// every mutable field is behind an `Arc`.
    pub(crate) fn launch_sessions(&self) -> LaunchSessions {
        LaunchSessions {
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            staging_base_dir: self.staging_base_dir.clone(),
            peer_routing: self.peer_routing.clone(),
            claude_cli_manager: Arc::clone(&self.claude_cli_manager),
            sandbox_manager: Arc::clone(&self.sandbox_manager),
            task_registry: self.task_registry.clone(),
            session_stdio: Arc::clone(&self.session_stdio),
            agent_activity_hub: Arc::clone(&self.agent_activity_hub),
            agent_roster: self.agent_roster(),
            host: Arc::new(self.clone()),
        }
    }
}
