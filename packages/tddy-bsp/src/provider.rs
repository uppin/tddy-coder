//! The enriched [`tddy_core::session_catalog::BuildCatalogProvider`] over `tddy-build` discovery.
//!
//! `tddy-core` owns the port and has no `tddy-build` dependency; the concrete provider lives here and
//! projects each `BUILD.yaml` target into a rich catalog entry. Registered on worktree-open by the
//! session owner (tddy-coder / daemon).

use std::path::Path;
use std::sync::Arc;

use tddy_build::capabilities::BuildMode;
use tddy_build::lower::lower_target;
use tddy_core::session_catalog::{
    register_build_catalog_provider, BuildCatalogProvider, BuildTargetCatalogEntry,
    CatalogCapabilities,
};

use crate::plugins::plugin_registry;

struct TddyBuildCatalogProvider;

impl BuildCatalogProvider for TddyBuildCatalogProvider {
    fn discover(&self, repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String> {
        let manifests = tddy_build::discovery::discover_build_manifests(repo_root)
            .map_err(|e| e.to_string())?;
        let registry = plugin_registry();
        let mut entries = Vec::new();
        for (manifest_path, manifest) in manifests {
            let source_path = manifest_path.display().to_string();
            let base_dir = manifest_base_dir(repo_root, &manifest_path);
            for target in manifest.targets {
                let package = target.id.split(':').next().unwrap_or("").to_string();
                let target_type = target.config.as_ref().map(|c| c.r#type.clone());
                let meta = tddy_build::capabilities::resolve_target_metadata(&target);

                // Derive sources/outputs by lowering the compile actions. A target that fails to
                // lower (e.g. a not-yet-registered type) still lists — with empty sources/outputs —
                // rather than aborting the whole discovery.
                let (sources, outputs) = match lower_target(&target, BuildMode::Compile, &registry)
                {
                    Ok(actions) => collect_sources_outputs(&actions),
                    Err(_) => (Vec::new(), Vec::new()),
                };

                entries.push(BuildTargetCatalogEntry {
                    id: target.id,
                    name: target.name,
                    package,
                    target_type,
                    base_dir: base_dir.clone(),
                    tags: meta.tags,
                    languages: meta.languages,
                    deps: target.deps,
                    sources,
                    outputs,
                    capabilities: CatalogCapabilities {
                        compile: meta.capabilities.compile,
                        test: meta.capabilities.test,
                        run: meta.capabilities.run,
                        debug: meta.capabilities.debug,
                    },
                    source_path: source_path.clone(),
                });
            }
        }
        Ok(entries)
    }
}

/// The directory of `manifest_path`, relative to `repo_root` (e.g. `packages/foo`). `None` when the
/// manifest sits at the repo root or the relative path cannot be computed.
fn manifest_base_dir(repo_root: &Path, manifest_path: &Path) -> Option<String> {
    let dir = manifest_path.parent()?;
    let rel = dir.strip_prefix(repo_root).ok()?;
    let rel = rel.to_string_lossy();
    if rel.is_empty() {
        None
    } else {
        Some(rel.into_owned())
    }
}

/// Collect the union of the lowered actions' input globs (sources) and declared output paths.
fn collect_sources_outputs(
    actions: &[tddy_build::proto::BuildAction],
) -> (Vec<String>, Vec<String>) {
    let mut sources = Vec::new();
    let mut outputs = Vec::new();
    for action in actions {
        for file_set in &action.inputs {
            for include in &file_set.include {
                if !sources.contains(include) {
                    sources.push(include.clone());
                }
            }
        }
        for output in &action.outputs {
            if !outputs.contains(&output.path) {
                outputs.push(output.path.clone());
            }
        }
    }
    (sources, outputs)
}

/// Register the build-catalog provider (idempotent — first registration wins).
pub fn register_catalog_provider() {
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

    /// The provider projects the full BSP metadata per target: `config.type`, resolved
    /// tags/languages/capabilities, the manifest's base directory, and the lowered action inputs as
    /// sources.
    #[test]
    fn enriches_targets_with_type_capabilities_tags_languages_and_sources() {
        // Given — a rust_library declaring srcs, tags, languages, and capabilities.
        let repo = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(repo.path().join("packages/foo/src")).expect("mkdir");
        std::fs::write(
            repo.path().join("packages/foo/BUILD.yaml"),
            "schema_version: 1\n\
             targets:\n\
             \x20 - id: \"packages/foo:lib\"\n\
             \x20   name: Foo library\n\
             \x20   tags: [library]\n\
             \x20   languages: [rust]\n\
             \x20   deps: [\"packages/core:lib\"]\n\
             \x20   config:\n\
             \x20     type: rust_library\n\
             \x20     package: foo\n\
             \x20     srcs: [\"packages/foo/src/lib.rs\"]\n",
        )
        .expect("write BUILD.yaml");

        // When
        let entries = TddyBuildCatalogProvider
            .discover(repo.path())
            .expect("discover must succeed");
        let lib = entries
            .iter()
            .find(|e| e.id == "packages/foo:lib")
            .expect("lib target present");

        // Then — the rich projection is populated.
        assert_eq!(lib.target_type.as_deref(), Some("rust_library"));
        assert_eq!(lib.base_dir.as_deref(), Some("packages/foo"));
        assert_eq!(lib.tags, vec!["library".to_string()]);
        assert_eq!(lib.languages, vec!["rust".to_string()]);
        assert_eq!(lib.deps, vec!["packages/core:lib".to_string()]);
        assert!(lib.capabilities.compile && lib.capabilities.test);
        assert!(!lib.capabilities.run, "a library is not runnable");
        assert!(
            lib.sources.contains(&"packages/foo/src/lib.rs".to_string()),
            "expected the declared srcs among sources, got {:?}",
            lib.sources
        );
    }
}
