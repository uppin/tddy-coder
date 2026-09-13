//! tddy-bsp: the BSP-shaped build server.
//!
//! Owns the build-target surface extracted out of `tddy-coder`:
//! - [`service::BspServiceImpl`] — the `bsp.BspService` RPC implementation (enumerate targets,
//!   sources/output paths, reload, compile/test/run) served over the workspace's protobuf/Connect +
//!   LiveKit transports.
//! - [`provider`] — the enriched [`tddy_core::session_catalog::BuildCatalogProvider`] that projects
//!   `BUILD.yaml` targets (capabilities/tags/languages/sources/outputs/deps) into the per-session
//!   catalog.
//! - [`plugins::plugin_registry`] — the build-plugin set (`tddy-build` knows no target types; this
//!   crate chooses them), used both for source/output derivation and compile/test/run execution.
//!
//! `tddy-coder` and the daemon depend on this crate, register the provider on worktree-open, and mount
//! the service. Feature: `docs/ft/coder/bsp-build-server.md`.

pub mod bsp_service;
pub mod plugins;
pub mod provider;
pub mod service;

pub use plugins::plugin_registry;
pub use provider::register_catalog_provider;
pub use service::BspServiceImpl;

/// The `bsp.BspService` entry the daemon's wiring layer registers.
///
/// `#unbundle` node 3 moved this service out of `tddy-daemon` and into the crate that already owns
/// both the BSP implementation and `plugins::plugin_registry` — the same five-plugin set
/// `tddy-tools`' `build_cli` duplicates verbatim, which node 5 then deletes.
///
/// `session_paths` stays injected: resolving a `(session_token, session_id)` to a worktree needs
/// the daemon's token/user/sessions-base machinery, which is wiring rather than build-server
/// behaviour.
pub fn build_bsp_service_entry(
    session_paths: bsp_service::SessionPathsResolver,
    tddy_data_dir: std::path::PathBuf,
) -> tddy_rpc::ServiceEntry {
    let server = tddy_service::BspServiceServer::new(bsp_service::DaemonBspService::new(
        session_paths,
        tddy_data_dir,
    ));
    tddy_rpc::ServiceEntry {
        name: "bsp.BspService",
        service: std::sync::Arc::new(server) as std::sync::Arc<dyn tddy_rpc::RpcService>,
    }
}

#[cfg(test)]
mod unbundle_service_entry_tests {
    use std::sync::Arc;

    /// A resolver no test here calls: the entry's name is decided at construction, not per request.
    fn an_unused_session_path_resolver() -> super::bsp_service::SessionPathsResolver {
        Arc::new(|_token: &str, _session_id: &str| {
            Err(tddy_rpc::Status::unauthenticated(
                "invalid or expired session",
            ))
        })
    }

    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        // Given — the entry the daemon's wiring layer builds
        let entry = super::build_bsp_service_entry(
            an_unused_session_path_resolver(),
            std::path::PathBuf::from("/unused"),
        );

        // Then
        assert_eq!(entry.name, "bsp.BspService");
    }
}
