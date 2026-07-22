//! Build-target discovery port.
//!
//! `tddy-core` deliberately has **no** dependency on `tddy-build`. It defines the
//! [`BuildCatalogProvider`] trait and a process-global registry; the binary that owns a session
//! (`tddy-coder`) registers a concrete provider on top of `tddy-build`'s `discover_build_manifests`.
//! Mirrors [`crate::toolcall::build`].

use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::entry::BuildTargetCatalogEntry;

/// Discovers the repository's `BUILD.yaml` build targets as catalog entries.
pub trait BuildCatalogProvider: Send + Sync {
    /// Discover build targets under `repo_root`. Errors are surfaced as a message string.
    fn discover(&self, repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String>;
}

static REGISTERED: OnceLock<Arc<dyn BuildCatalogProvider>> = OnceLock::new();

/// Register the process-wide build-catalog provider. The first registration wins.
pub fn register_build_catalog_provider(provider: Arc<dyn BuildCatalogProvider>) {
    let _ = REGISTERED.set(provider);
}

/// The registered provider, if any. `None` means build targets are simply absent from the catalog
/// (graceful — not a data fallback).
pub fn build_catalog_provider() -> Option<Arc<dyn BuildCatalogProvider>> {
    REGISTERED.get().cloned()
}
