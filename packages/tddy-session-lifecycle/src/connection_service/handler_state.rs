//! The host state an RPC-family handler above this crate is built from.
//!
//! `tddy-daemon-rpc` builds each family's handler with `from_host(&DaemonSessionHost)`, holding
//! only the fields that family reads. These hand those fields out: the shared ones (`Arc`s, and
//! components built on them) as clones of the same handle, so a handler talks to the peers, spawn
//! client, common room, registry, token store, idle tracker, task registry and jails the host does
//! rather than to copies of them.

use std::path::Path;
use std::sync::Arc;

use livekit::prelude::Room;
use tddy_model_registry::ModelRegistryStore;
use tddy_spawn::spawn_worker::SpawnClient;

use super::{DaemonSessionHost, LocalExecTools};
use crate::config::DaemonConfig;
use crate::multi_host::EligibleDaemonSource;
use crate::peer_routing::PeerRouting;
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
        Arc::clone(self.peer_routing.eligible_daemon_source())
    }

    /// The forked spawn worker, when this daemon runs one.
    #[must_use]
    pub fn spawn_client(&self) -> Option<Arc<SpawnClient>> {
        self.spawn_client.clone()
    }

    /// The common-room LiveKit slot a request is forwarded to a peer through, when configured.
    #[must_use]
    pub fn common_room_livekit_room(&self) -> Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>> {
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
            Arc::clone(&self.hosted_agent_clones),
        )
    }
}
