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
pub fn build_bsp_service_entry() -> tddy_rpc::ServiceEntry {
    // TODO(sandbox-spawn-services): implement
    unimplemented!("build_bsp_service_entry")
}

#[cfg(test)]
mod unbundle_service_entry_tests {
    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        assert_eq!(super::build_bsp_service_entry().name, "bsp.BspService");
    }
}
