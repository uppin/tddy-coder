//! Catalog entry shape (stored as a JSON blob) and the `package` projection.

use serde::{Deserialize, Serialize};

/// Which kind of thing a catalog entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogEntryKind {
    /// A declarative YAML action manifest ([`crate::session_actions`]).
    ActionManifest,
    /// A `BUILD.yaml` build target, auto-discovered via [`super::provider::BuildCatalogProvider`].
    BuildTarget,
}

impl CatalogEntryKind {
    /// The string stored in the `kind` column / JSON (`"action_manifest"` | `"build_target"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogEntryKind::ActionManifest => "action_manifest",
            CatalogEntryKind::BuildTarget => "build_target",
        }
    }
}

/// One row of the catalog. Serialized as the `json` column; the read path reconstructs an
/// [`crate::session_actions::ActionSummary`] from it (of which this is a superset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub kind: CatalogEntryKind,
    /// Manifest `id`, or a build target id such as `packages/foo:binary`.
    pub id: String,
    /// Projected index key. `BuildTarget`: id prefix before `:`. `ActionManifest`: parent dir of `path`.
    pub package: String,
    /// Manifest `summary`, or the build target `name`.
    pub summary: String,
    /// Manifest rel-path without extension (the `--action` handle), or the build target id.
    pub path: String,
    pub has_input_schema: bool,
    pub has_output_schema: bool,
    /// Provenance: absolute manifest path, or absolute `BUILD.yaml` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// The lifecycle capabilities of a build target (BSP `BuildTargetCapabilities`), as primitive bools
/// so `tddy-core` needs no `tddy-build` dependency. Intentionally mirrors `tddy_build`'s
/// `TargetCapabilities` field-for-field; the provider maps across the crate boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogCapabilities {
    pub compile: bool,
    pub test: bool,
    pub run: bool,
    pub debug: bool,
}

/// A build target handed across the [`super::provider::BuildCatalogProvider`] port.
///
/// Deliberately free of `tddy-build` types so `tddy-core` keeps no dependency on it. Carries the rich
/// projection the BSP layer needs; primitive fields only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTargetCatalogEntry {
    /// Build target id, e.g. `packages/foo:binary`.
    pub id: String,
    /// Build target `name`.
    pub name: String,
    /// Projected package (id prefix before `:`).
    pub package: String,
    /// `config.type` tag, e.g. `rust_library`.
    pub target_type: Option<String>,
    /// Directory of the `BUILD.yaml`, relative to the repo root.
    pub base_dir: Option<String>,
    /// Resolved tags (declared or derived).
    pub tags: Vec<String>,
    /// Resolved language ids (declared or derived).
    pub languages: Vec<String>,
    /// The target's declared `deps`.
    pub deps: Vec<String>,
    /// Source globs (union of the lowered actions' input globs).
    pub sources: Vec<String>,
    /// Declared output paths (union of the lowered actions' outputs).
    pub outputs: Vec<String>,
    /// Resolved lifecycle capabilities.
    pub capabilities: CatalogCapabilities,
    /// Absolute path of the `BUILD.yaml` this target came from.
    pub source_path: String,
}

/// Derive the projected `package` for an entry.
///
/// - [`CatalogEntryKind::BuildTarget`]: the substring of `id_or_path` before the first `:`.
/// - [`CatalogEntryKind::ActionManifest`]: the parent directory of `id_or_path` (a top-level path
///   with no directory projects to the empty string).
pub fn project_package(kind: CatalogEntryKind, id_or_path: &str) -> String {
    match kind {
        CatalogEntryKind::BuildTarget => id_or_path.split(':').next().unwrap_or("").to_string(),
        CatalogEntryKind::ActionManifest => match id_or_path.rfind('/') {
            Some(i) => id_or_path[..i].to_string(),
            None => String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_target_id_projects_to_the_package_before_the_colon() {
        // Given / When
        let package = project_package(CatalogEntryKind::BuildTarget, "packages/foo:binary");

        // Then
        assert_eq!(package, "packages/foo");
    }

    #[test]
    fn an_action_manifest_path_projects_to_its_parent_directory() {
        // Given / When
        let package = project_package(CatalogEntryKind::ActionManifest, "packages/foo/build");

        // Then
        assert_eq!(package, "packages/foo");
    }

    #[test]
    fn a_top_level_manifest_path_projects_to_an_empty_package() {
        // Given — a manifest at the discovery root with no directory component.
        // When
        let package = project_package(CatalogEntryKind::ActionManifest, "run-tests");

        // Then — no package scope.
        assert_eq!(package, "");
    }
}
