//! Where extensions live: `<tddy data dir>/extensions` (R1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::ExtensionError;

/// The extensions directory: `installed/<id>/` per extension plus `index.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionsDir {
    root: PathBuf,
}

impl ExtensionsDir {
    /// The extensions directory under an already resolved tddy data dir: `<data_dir>/extensions`.
    pub fn under_data_dir(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("extensions"),
        }
    }

    /// Resolves the extensions directory from this process's environment: `TDDY_DATA_DIR`, else
    /// `$HOME/.tddy`. Only the CLI calls this; library callers pass a directory.
    pub fn resolve() -> Result<Self, ExtensionError> {
        Self::resolve_from(std::env::var_os("TDDY_DATA_DIR"), std::env::var_os("HOME"))
    }

    /// The resolution rule with its inputs passed in, as the daemon resolves its data dir: a
    /// non-blank `tddy_data_dir` wins; else the debug-build default (`tmp/.tddy`, none in release);
    /// else `<home>/.tddy`. Then `<that>/extensions`. Nothing set is [`ExtensionError::NoDataDir`].
    pub fn resolve_from(
        tddy_data_dir: Option<OsString>,
        home: Option<OsString>,
    ) -> Result<Self, ExtensionError> {
        let data_dir = non_blank(tddy_data_dir)
            .or_else(tddy_session_store::output::default_tddy_data_dir)
            .or_else(|| non_blank(home).map(|home| home.join(".tddy")))
            .ok_or(ExtensionError::NoDataDir)?;
        Ok(Self::under_data_dir(&data_dir))
    }

    /// The extensions directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/installed` — one subdirectory per installed extension.
    pub fn installed(&self) -> PathBuf {
        self.root.join("installed")
    }

    /// `<root>/installed/<id>` — where extension `id` lives once installed.
    pub fn extension(&self, id: &str) -> PathBuf {
        self.installed().join(id)
    }

    /// `<root>/index.json`.
    pub fn index(&self) -> PathBuf {
        self.root.join("index.json")
    }
}

/// `value` as a path, unless it is unset or blank: a blank variable is read as unset, as the daemon
/// reads `TDDY_DATA_DIR`.
fn non_blank(value: Option<OsString>) -> Option<PathBuf> {
    value
        .filter(|value| !value.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
}
