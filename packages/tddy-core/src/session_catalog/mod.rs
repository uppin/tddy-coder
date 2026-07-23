//! Per-session SQLite catalog: the single source of truth for what is listable in a session.
//!
//! Unifies two entry kinds into one indexed store at `<session_dir>/catalog.db`:
//! - **action manifests** (the YAML actions of [`crate::session_actions`]), and
//! - **build targets** ("tddy targets"), auto-discovered from `BUILD.yaml` and supplied through the
//!   [`provider::BuildCatalogProvider`] port (so `tddy-core` needs no dependency on `tddy-build`).
//!
//! Each entry is stored as a JSON blob; a projected `package` column serves the first index
//! ("list targets per package"). Population runs as a [`tddy_task`] on worktree-open, and the first
//! list query blocks until that populate task is terminal.
//!
//! Feature: `docs/ft/coder/session-catalog.md`.

pub mod entry;
pub mod error;
pub mod populate;
pub mod provider;
pub mod read;
pub mod store;

pub use entry::{
    project_package, BuildTargetCatalogEntry, CatalogCapabilities, CatalogEntry, CatalogEntryKind,
};
pub use error::CatalogError;
pub use populate::PopulateCatalogTask;
pub use provider::{build_catalog_provider, register_build_catalog_provider, BuildCatalogProvider};
pub use read::{session_catalog, SessionCatalog};
pub use store::{list_build_targets, list_build_targets_for_package, BuildTargetSummary};
