//! Every refusal names the extension and, where there is one, the file (R3). `file` and `entry`
//! paths are relative to the extension's root — the layout a reader recognises, not the partial
//! directory the install happened to unpack into.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionError {
    #[error("extension {extension}: no {}", file.display())]
    ManifestMissing { extension: String, file: PathBuf },

    #[error("extension {extension}: {} does not parse: {message}", file.display())]
    ManifestUnparsable {
        extension: String,
        file: PathBuf,
        message: String,
    },

    #[error("extension {requested}: {} declares id {found}", file.display())]
    IdMismatch {
        requested: String,
        found: String,
        file: PathBuf,
    },

    #[error(
        "extension {extension}: {} declares schema_version {schema_version}, supported 0..=1",
        file.display()
    )]
    SchemaVersionOutOfRange {
        extension: String,
        file: PathBuf,
        schema_version: i32,
    },

    #[error(
        "extension {extension}: archive entry {} is absolute, climbs out, or links outside the extension",
        entry.display()
    )]
    UnsafeArchiveEntry { extension: String, entry: PathBuf },

    #[error("extension {extension}: grammar {grammar} is declared but {} is not shipped", file.display())]
    GrammarWasmMissing {
        extension: String,
        grammar: String,
        file: PathBuf,
    },

    #[error("extension {extension}: language directory has no {}", file.display())]
    LanguageConfigMissing { extension: String, file: PathBuf },

    #[error("extension {extension}: registry answered {status} for {url}")]
    RegistryStatus {
        extension: String,
        url: String,
        status: u16,
    },

    #[error("extension {extension}: request to {url} failed: {message}")]
    RegistryUnreachable {
        extension: String,
        url: String,
        message: String,
    },

    #[error("extension {extension} is not installed")]
    NotInstalled { extension: String },

    #[error("neither TDDY_DATA_DIR nor HOME is set; cannot locate the extensions directory")]
    NoDataDir,

    #[error("{}: {message}", path.display())]
    Io { path: PathBuf, message: String },

    #[error("{id:?} is not a valid extension id: it must be one name, such as demo-lang")]
    InvalidExtensionId { id: String },
}

impl ExtensionError {
    /// An I/O failure at `path`.
    pub(crate) fn io(path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}
