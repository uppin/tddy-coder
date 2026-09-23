//! Family A — `catalog.CatalogService`.

mod agent_models;
mod ports;
mod subagent_row;

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_model_registry::ModelRegistryStore;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::relay_idle::RpcActivity;

/// The four catalogue RPCs: the configured tools and agents, the model registry's assistants, and
/// the idle tracker every call bumps.
pub struct CatalogRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    model_registry: Option<Arc<ModelRegistryStore>>,
    rpc_activity: RpcActivity,
}

impl CatalogRpcHandler {
    /// A handler sharing `host`'s state — the same model registry and the same idle tracker.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            model_registry: host.model_registry(),
            rpc_activity: host.rpc_activity(),
        }
    }
}
