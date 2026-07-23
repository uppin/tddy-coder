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
