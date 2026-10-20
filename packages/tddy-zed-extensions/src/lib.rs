//! Zed editor extensions for tddy: install, list and remove them under
//! `<tddy data dir>/extensions`, in Zed's own installed layout, from Zed's extension registry or a
//! local directory. Nothing here loads an installed extension — that is `extension-outline`'s.
//!
//! Library functions take the [`ExtensionsDir`] and the [`Registry`] explicitly; only
//! [`cli::run`] resolves them from the environment (`TDDY_DATA_DIR`, `TDDY_ZED_EXTENSIONS_API`).

pub mod cli;
pub mod dir;
pub mod error;
pub mod index;
pub mod install;
pub mod manifest;
pub mod registry;

pub use dir::ExtensionsDir;
pub use error::ExtensionError;
pub use index::{ExtensionSource, Index, IndexEntry};
pub use install::{
    install_from_path, install_from_registry, list, remove, InstallOutcome, InstalledExtension,
};
pub use manifest::{ExtensionManifest, GrammarEntry, LanguageServerEntry, LibEntry, LibKind};
pub use registry::Registry;
