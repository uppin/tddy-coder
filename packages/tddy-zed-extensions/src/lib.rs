//! Zed editor extensions for tddy: install, list and remove them under
//! `<tddy data dir>/extensions`, in Zed's own installed layout, from Zed's extension registry or a
//! local directory.
//!
//! An installed extension's languages become language supports ([`support`]): each claims its
//! `path_suffixes`, loads its grammar from `grammars/<grammar>.wasm` ([`grammar`]) and answers item
//! anchors from its `outline.scm` ([`outline`]), with no language server.
//!
//! Library functions take the [`ExtensionsDir`] and the [`Registry`] explicitly; only
//! [`cli::run`] resolves them from the environment (`TDDY_DATA_DIR`, `TDDY_ZED_EXTENSIONS_API`).

pub mod cli;
pub mod dir;
pub mod error;
pub mod grammar;
pub mod index;
pub mod install;
pub mod language;
pub mod manifest;
pub mod outline;
pub mod registry;
pub mod support;

mod archive;
mod staging;

pub use dir::ExtensionsDir;
pub use error::ExtensionError;
pub use grammar::{GrammarLoadError, GrammarStore};
pub use index::{ExtensionSource, Index, IndexEntry};
pub use install::{
    install_from_path, install_from_registry, list, remove, InstallOutcome, InstalledExtension,
};
pub use language::{ExtensionLanguage, LanguageConfig};
pub use manifest::{ExtensionManifest, GrammarEntry, LanguageServerEntry, LibEntry, LibKind};
pub use outline::{Outline, OutlineEntry};
pub use registry::Registry;
pub use support::{
    load_installed, ExtensionSupport, InstalledExtensions, InstalledLanguages, OutlineBackend,
};
