//! Build-system extension point.
//!
//! `tddy-core` deliberately has **no** dependency on `tddy-build`. It defines the
//! [`BuildExecutor`] trait and a process-global registry; the binary that owns a
//! session (`tddy-coder`) registers a concrete executor before starting the
//! toolcall listener, so relayed `build` / `build-list` requests (via
//! `TDDY_SOCKET`) can be served co-located with the worktree.

use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Filters for a `build-list` request.
#[derive(Debug, Clone, Default)]
pub struct BuildListQuery {
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: usize,
}

/// Options for a `build` request.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    pub no_cache: bool,
    pub dry_run: bool,
}

/// Serves `build` / `build-list` against a repository. Implemented in `tddy-coder`
/// on top of `tddy-build`. Methods are synchronous; implementations that need an
/// async runtime should block internally (the listener calls them off the async
/// executor via `spawn_blocking`).
pub trait BuildExecutor: Send + Sync {
    /// List build targets; returns `{"targets":[…],"total":N,…}`.
    fn build_list(
        &self,
        repo_dir: &Path,
        query: &BuildListQuery,
    ) -> Result<serde_json::Value, String>;

    /// Build a target; returns the build record (`status`, `target`, `actions`).
    fn build(
        &self,
        repo_dir: &Path,
        target: &str,
        opts: &BuildOptions,
    ) -> Result<serde_json::Value, String>;
}

static REGISTERED: OnceLock<Arc<dyn BuildExecutor>> = OnceLock::new();

/// Register the process-wide build executor. The first registration wins.
pub fn register_build_executor(executor: Arc<dyn BuildExecutor>) {
    let _ = REGISTERED.set(executor);
}

/// The registered executor, if any.
pub fn build_executor() -> Option<Arc<dyn BuildExecutor>> {
    REGISTERED.get().cloned()
}
