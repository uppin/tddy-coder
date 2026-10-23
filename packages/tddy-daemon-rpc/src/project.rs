//! Family D — `project.ProjectService`.

mod clone_destination;
mod coordinate_handlers;
mod coordinate_route;
mod entries;
mod home_agent_handler;
mod ports;

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_model_registry::ModelRegistryStore;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_spawn::spawn_worker::SpawnClient;

/// The project RPCs, holding only what they read: the caller's identity, the projects store under
/// `tddy_data_dir`, the peers a listing fans out to, the spawn client a clone runs through, and the
/// model registry the agent catalog a home agent is checked against is built from.
pub struct ProjectRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    spawn_client: Option<Arc<SpawnClient>>,
    common_room_livekit_room: Option<CommonRoom>,
    model_registry: Option<Arc<ModelRegistryStore>>,
}

impl ProjectRpcHandler {
    /// A handler sharing `host`'s state — the same peer source, spawn client and room slot.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            eligible_daemon_source: host.eligible_daemon_source(),
            spawn_client: host.spawn_client(),
            common_room_livekit_room: host.common_room_livekit_room(),
            model_registry: host.model_registry(),
        }
    }
}
