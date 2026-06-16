//! Discover `BUILD.yaml` manifests across a repository.

use std::path::{Path, PathBuf};

use crate::error::BuildError;
use crate::manifest::{load_build_manifest, BuildManifest};

const MANIFEST_GLOBS: &[&str] = &[
    "**/BUILD.yaml",
    "**/BUILD.yml",
    "**/build.yaml",
    "**/build.yml",
];

/// Glob `**/{BUILD,build}.{yaml,yml}` under `repo_root` and parse each match into
/// a [`BuildManifest`], returning the manifest paired with its source path. Paths
/// are returned sorted for determinism.
pub fn discover_build_manifests(
    repo_root: &Path,
) -> Result<Vec<(PathBuf, BuildManifest)>, BuildError> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for pattern in MANIFEST_GLOBS {
        let joined = repo_root.join(pattern);
        let entries = glob::glob(&joined.to_string_lossy())
            .map_err(|e| BuildError::Io(format!("bad glob {pattern}: {e}")))?;
        for entry in entries.flatten() {
            if entry.is_file() {
                paths.push(entry);
            }
        }
    }
    paths.sort();
    paths.dedup();

    log::debug!("discovered {} build manifest(s)", paths.len());

    let mut manifests = Vec::with_capacity(paths.len());
    for path in paths {
        log::trace!("build manifest: {}", path.display());
        let yaml = std::fs::read_to_string(&path)
            .map_err(|e| BuildError::Io(format!("{}: {}", path.display(), e)))?;
        let manifest = load_build_manifest(&yaml)
            .map_err(|e| BuildError::Manifest(format!("{}: {}", path.display(), e)))?;
        manifests.push((path, manifest));
    }
    Ok(manifests)
}
