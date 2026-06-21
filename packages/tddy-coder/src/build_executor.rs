//! Registers a `tddy-build`-backed [`tddy_core::toolcall::BuildExecutor`] so that
//! relayed `build` / `build-list` requests (via `TDDY_SOCKET`) are served
//! co-located with the worktree.
//!
//! `tddy-core` owns the extension point and has no `tddy-build` dependency; the
//! dependency lives here, in the binary that owns the session.

use std::path::Path;
use std::sync::Arc;

use tddy_build::plugin::PluginRegistry;
use tddy_core::toolcall::{register_build_executor, BuildExecutor, BuildListQuery, BuildOptions};

struct TddyBuildExecutor;

/// Assemble the build-plugin registry from the recipe crates. `tddy-build` knows no
/// target types; this binary chooses the plugin set.
fn plugin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(tddy_build_rust::RustPlugin));
    registry.register(Arc::new(tddy_build_typescript::TypeScriptPlugin));
    registry.register(Arc::new(tddy_build_docker::DockerPlugin));
    registry.register(Arc::new(tddy_build_buildroot::BuildrootPlugin));
    registry.register(Arc::new(tddy_build_qemu::QemuPlugin));
    registry
}

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
                &registry,
            ))
            .map_err(|e| e.to_string())
    }
}

/// Register the build executor (idempotent — first registration wins).
pub fn register() {
    register_build_executor(Arc::new(TddyBuildExecutor));
}

#[cfg(test)]
mod tests {
    use super::plugin_registry;

    /// `BuildrootPlugin` and `QemuPlugin` must be registered in `tddy-coder`'s plugin
    /// registry so that build relay requests for `buildroot_image` and `qemu_disk_image`
    /// targets are handled without "unknown target type" errors.
    ///
    /// RED: fails until both plugins are added to `plugin_registry()`.
    #[test]
    fn buildroot_and_qemu_plugins_registered_in_coder_registry() {
        let registry = plugin_registry();
        let types: Vec<&str> = registry.registered_types().collect();
        assert!(
            types.contains(&"buildroot_image"),
            "buildroot_image plugin must be registered in tddy-coder; registered: {types:?}"
        );
        assert!(
            types.contains(&"qemu_disk_image"),
            "qemu_disk_image plugin must be registered in tddy-coder; registered: {types:?}"
        );
    }
}
