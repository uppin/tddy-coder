//! The build-plugin registry. `tddy-build` carries no target-type knowledge; this crate chooses the
//! ecosystem recipes and assembles them, for both source/output derivation and build execution.

use std::sync::Arc;

use tddy_build::plugin::PluginRegistry;

/// Assemble the build-plugin registry from the recipe crates.
pub fn plugin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(tddy_build_rust::RustPlugin));
    registry.register(Arc::new(tddy_build_typescript::TypeScriptPlugin));
    registry.register(Arc::new(tddy_build_docker::DockerPlugin));
    registry.register(Arc::new(tddy_build_buildroot::BuildrootPlugin));
    registry.register(Arc::new(tddy_build_qemu::QemuPlugin));
    registry
}

#[cfg(test)]
mod tests {
    use super::plugin_registry;

    /// Every ecosystem recipe must be registered so build/BSP requests for their target types are
    /// handled without "unknown target type" errors.
    #[test]
    fn registers_every_ecosystem_plugin_type() {
        let registry = plugin_registry();
        let types: Vec<&str> = registry.registered_types().collect();
        for expected in [
            "rust_binary",
            "rust_library",
            "typescript",
            "docker_image",
            "buildroot_image",
            "qemu_disk_image",
        ] {
            assert!(
                types.contains(&expected),
                "{expected} plugin must be registered; registered: {types:?}"
            );
        }
    }
}
