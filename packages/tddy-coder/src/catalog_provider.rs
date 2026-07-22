//! Registers a `tddy-build`-backed [`tddy_core::session_catalog::BuildCatalogProvider`] so the
//! per-session catalog is populated with the repository's `BUILD.yaml` build targets.
//!
//! `tddy-core` owns the extension point and has no `tddy-build` dependency; the dependency lives
//! here, in the binary that owns the session (mirrors [`crate::build_executor`]).

use std::path::Path;
use std::sync::Arc;

use tddy_core::session_catalog::{
    register_build_catalog_provider, BuildCatalogProvider, BuildTargetCatalogEntry,
};

struct TddyBuildCatalogProvider;

impl BuildCatalogProvider for TddyBuildCatalogProvider {
    fn discover(&self, repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String> {
        let manifests = tddy_build::discovery::discover_build_manifests(repo_root)
            .map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        for (manifest_path, manifest) in manifests {
            let source_path = manifest_path.display().to_string();
            for target in manifest.targets {
                let package = target.id.split(':').next().unwrap_or("").to_string();
                entries.push(BuildTargetCatalogEntry {
                    id: target.id,
                    name: target.name,
                    package,
                    source_path: source_path.clone(),
                });
            }
        }
        Ok(entries)
    }
}

/// Register the build-catalog provider (idempotent — first registration wins).
pub fn register() {
    register_build_catalog_provider(Arc::new(TddyBuildCatalogProvider));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Discovery flattens every `BUILD.yaml` target into a catalog entry, projecting `package`
    /// from the id prefix (before `:`) and recording the manifest path as `source_path`.
    #[test]
    fn discovers_build_targets_from_build_yaml_with_package_projected_from_the_id() {
        // Given — a repo with a BUILD.yaml declaring two targets under `packages/foo`.
        let repo = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(repo.path().join("packages/foo")).expect("mkdir");
        std::fs::write(
            repo.path().join("packages/foo/BUILD.yaml"),
            "schema_version: 1\n\
             targets:\n\
             \x20 - id: \"packages/foo:binary\"\n\
             \x20   name: Foo binary\n\
             \x20 - id: \"packages/foo:test\"\n\
             \x20   name: Foo tests\n",
        )
        .expect("write BUILD.yaml");

        // When
        let entries = TddyBuildCatalogProvider
            .discover(repo.path())
            .expect("discover must succeed");

        // Then — both targets, in declaration order, package projected from the id prefix.
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["packages/foo:binary", "packages/foo:test"]);
        assert_eq!(entries[0].package, "packages/foo");
        assert_eq!(entries[0].name, "Foo binary");
        assert!(
            entries[0].source_path.ends_with("packages/foo/BUILD.yaml"),
            "source_path must point at the BUILD.yaml, was {}",
            entries[0].source_path
        );
    }
}
