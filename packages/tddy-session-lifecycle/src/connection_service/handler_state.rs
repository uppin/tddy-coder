//! The host state an RPC-family handler above this crate is built from.
//!
//! `tddy-daemon-rpc` builds each family's handler with `from_host(&DaemonSessionHost)`, holding
//! only the fields that family reads. These hand those fields out: the shared ones (`Arc`s, and
//! components built on them) as clones of the same handle, so a handler talks to the peers, spawn
//! client, common room, registry and idle tracker the host does rather than to copies of them.

use std::path::Path;
use std::sync::Arc;

use livekit::prelude::Room;
use tddy_model_registry::ModelRegistryStore;
use tddy_spawn::spawn_worker::SpawnClient;

use super::DaemonSessionHost;
use crate::config::DaemonConfig;
use crate::multi_host::EligibleDaemonSource;
use crate::relay_idle::RpcActivity;

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
        Arc::clone(&self.eligible_daemon_source)
    }

    /// The forked spawn worker, when this daemon runs one.
    #[must_use]
    pub fn spawn_client(&self) -> Option<Arc<SpawnClient>> {
        self.spawn_client.clone()
    }

    /// The common-room LiveKit slot a request is forwarded to a peer through, when configured.
    #[must_use]
    pub fn common_room_livekit_room(&self) -> Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>> {
        self.common_room_livekit_room.clone()
    }

    /// This daemon's model registry — the assistants an agent id may resolve to — when one is wired.
    #[must_use]
    pub fn model_registry(&self) -> Option<Arc<ModelRegistryStore>> {
        self.model_registry.clone()
    }

    /// The idle tracker this host bumps on every RPC, shared rather than copied, so a handler's
    /// calls keep the same relay alive the host's own do.
    #[must_use]
    pub fn rpc_activity(&self) -> RpcActivity {
        self.rpc_activity.clone()
    }
}
