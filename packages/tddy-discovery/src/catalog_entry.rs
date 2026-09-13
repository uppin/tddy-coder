//! Registers a [`catalog.CatalogService`] implementation on the `tddy-rpc` transport.
//!
//! The implementation lives in `tddy-daemon` (routing, config, registry probes); this crate owns
//! the catalogue domain and publishes the coordinate the stack moved family A to.

use std::sync::Arc;

use tddy_rpc::RpcService;
use tddy_service::proto::catalog::{CatalogService, CatalogServiceServer};

/// The coordinate `catalog.proto`'s four methods are served at.
pub const CATALOG_SERVICE: &str = "catalog.CatalogService";

/// The `catalog.CatalogService` entry a host's wiring layer registers.
#[must_use]
pub fn build_catalog_entry<S>(service: S) -> tddy_rpc::ServiceEntry
where
    S: CatalogService + Send + Sync + 'static,
{
    tddy_rpc::ServiceEntry {
        name: CATALOG_SERVICE,
        service: Arc::new(CatalogServiceServer::new(service)) as Arc<dyn RpcService>,
    }
}
