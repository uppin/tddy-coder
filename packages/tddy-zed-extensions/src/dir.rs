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

    /// The resolution rule with its inputs passed in: a non-blank `tddy_data_dir` wins, else
    /// `<home>/.tddy`; `<that>/extensions`. Neither set is [`ExtensionError::NoDataDir`].
    pub fn resolve_from(
        tddy_data_dir: Option<OsString>,
        home: Option<OsString>,
    ) -> Result<Self, ExtensionError> {
        let _ = (tddy_data_dir, home);
        // TODO(reshape-extension-install): implement
        todo!("ExtensionsDir::resolve_from")
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
