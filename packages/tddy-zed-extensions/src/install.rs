//! Install, list and remove (R1–R6).

use std::path::Path;

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::index::IndexEntry;
use crate::registry::Registry;

/// What an install did (R5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Nothing of this id was installed before.
    Installed(IndexEntry),
    /// A different version was installed and is now replaced.
    Replaced {
        previous_version: String,
        entry: IndexEntry,
    },
    /// The same version is already installed; nothing changed.
    AlreadyInstalled(IndexEntry),
}

/// One installed extension and what it provides, for `list` (R6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledExtension {
    pub entry: IndexEntry,
    /// Language names, from each `languages/<x>/config.toml`'s `name`.
    pub languages: Vec<String>,
    /// Grammar names declared in `extension.toml`.
    pub grammars: Vec<String>,
    /// Language-server ids declared in `extension.toml`.
    pub language_servers: Vec<String>,
    /// Whether `extension.wasm` is present.
    pub has_wasm: bool,
}

impl InstalledExtension {
    /// `<id> <version> <source> — languages: …; grammars: …; language servers: …; wasm: yes|no`,
    /// an empty list printed as `none`.
    pub fn line(&self) -> String {
        // TODO(reshape-extension-install): implement
        todo!("InstalledExtension::line")
    }
}

/// Downloads `id` (at `version`, else the latest compatible) from `registry` and installs it:
/// `GET {api}/extensions/{id}[/{version}]/download` with the R4 schema and WASM API range.
pub async fn install_from_registry(
    dir: &ExtensionsDir,
    registry: &Registry,
    id: &str,
    version: Option<&str>,
) -> Result<InstallOutcome, ExtensionError> {
    let _ = (dir, registry, id, version);
    // TODO(reshape-extension-install): implement
    todo!("install_from_registry")
}

/// Copies the extension directory at `path` (a dev extension) in, under its manifest's id.
pub fn install_from_path(
    dir: &ExtensionsDir,
    path: &Path,
) -> Result<InstallOutcome, ExtensionError> {
    let _ = (dir, path);
    // TODO(reshape-extension-install): implement
    todo!("install_from_path")
}

/// Every installed extension, sorted by id.
pub fn list(dir: &ExtensionsDir) -> Result<Vec<InstalledExtension>, ExtensionError> {
    let _ = dir;
    // TODO(reshape-extension-install): implement
    todo!("list")
}

/// Deletes `installed/<id>` and its index entry, returning the entry removed.
pub fn remove(dir: &ExtensionsDir, id: &str) -> Result<IndexEntry, ExtensionError> {
    let _ = (dir, id);
    // TODO(reshape-extension-install): implement
    todo!("remove")
}
