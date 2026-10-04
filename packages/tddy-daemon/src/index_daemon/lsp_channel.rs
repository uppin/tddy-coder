//! The managed index daemon as the channel the index-backed `Lsp*` executor asks.
//!
//! `tddy_lsp_executor::index_backed::IndexLspExecutor` cannot name [`IndexDaemonRegistry`] — this
//! crate depends on it, not the other way round — so it takes an [`IndexChannel`] port, and this is
//! the registry answering it: the same [`IndexDaemonRegistry::connect`] the code pane's navigation
//! forwards through, so a session's tools and the pane ask one index.

use tddy_lsp_executor::index_backed::IndexChannel;

use super::IndexDaemonRegistry;

#[async_trait::async_trait]
impl IndexChannel for IndexDaemonRegistry {
    async fn connect(&self) -> Result<tonic::transport::Channel, String> {
        IndexDaemonRegistry::connect(self)
            .await
            .map_err(|err| err.to_string())
    }
}
