//! Family D — `project.ProjectService`.

mod clone_destination;
mod coordinate_handlers;
mod coordinate_route;
mod entries;
mod ports;

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::multi_host::EligibleDaemonSource;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_spawn::spawn_worker::SpawnClient;
use tddy_spawn::supervisor_client::SpawnBackendChoice;

/// The project RPCs, holding only what they read: the caller's identity, the projects store under
/// `tddy_data_dir`, the peers a listing fans out to, and the spawn client a clone runs through.
pub struct ProjectRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    clone_backend: CloneBackend,
    common_room_livekit_room: Option<CommonRoom>,
}

/// Where a clone runs: the backend this daemon was started with, and the forked worker that backend
/// uses when it is not the supervisor. One field because the two are only ever read together.
struct CloneBackend {
    choice: SpawnBackendChoice,
    worker: Option<Arc<SpawnClient>>,
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
            clone_backend: CloneBackend {
                choice: host.spawn_backend(),
                worker: host.spawn_client(),
            },
            common_room_livekit_room: host.common_room_livekit_room(),
        }
    }
}
