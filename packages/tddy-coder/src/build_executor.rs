//! Registers a `tddy-build`-backed [`tddy_core::toolcall::BuildExecutor`] so that
//! relayed `build` / `build-list` requests (via `TDDY_SOCKET`) are served
//! co-located with the worktree.
//!
//! `tddy-core` owns the extension point and has no `tddy-build` dependency; the
//! dependency lives here, in the binary that owns the session.

use std::path::Path;
use std::sync::Arc;

use tddy_bsp::plugin_registry;
use tddy_core::toolcall::{register_build_executor, BuildExecutor, BuildListQuery, BuildOptions};

struct TddyBuildExecutor;

impl BuildExecutor for TddyBuildExecutor {
    fn build_list(
        &self,
        repo_dir: &Path,
        query: &BuildListQuery,
    ) -> Result<serde_json::Value, String> {
        let q = tddy_build::service::BuildListQuery {
            query: query.query.clone(),
            limit: query.limit,
            offset: query.offset,
        };
        tddy_build::service::build_list_json(repo_dir, &q).map_err(|e| e.to_string())
    }

    fn build(
        &self,
        repo_dir: &Path,
        target: &str,
        opts: &BuildOptions,
    ) -> Result<serde_json::Value, String> {
        // We are called from `spawn_blocking`; block on the async build path using
        // the surrounding runtime.
        let registry = plugin_registry();
        tokio::runtime::Handle::current()
            .block_on(tddy_build::service::build_json(
                repo_dir,
                target,
                opts.no_cache,
                opts.dry_run,
                tddy_build::BuildMode::Compile,
                &registry,
            ))
            .map_err(|e| e.to_string())
    }
}

/// Register the build executor (idempotent — first registration wins).
pub fn register() {
    register_build_executor(Arc::new(TddyBuildExecutor));
}
