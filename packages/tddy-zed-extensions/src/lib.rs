//! Zed editor extensions for tddy: install, list and remove them under
//! `<tddy data dir>/extensions`, in Zed's own installed layout, from Zed's extension registry or a
//! local directory.
//!
//! An installed extension's languages become language supports ([`support`]): each claims its
//! `path_suffixes`, loads its grammar from `grammars/<grammar>.wasm` ([`grammar`]) and answers item
//! anchors from its `outline.scm` ([`outline`]), with no language server — unless one of the
//! extension's `[language_servers.<id>]` names it and `<extensions dir>/settings.json` gives that
//! server a command ([`settings`], [`server`]): then it has `rename_symbol` and references too, and
//! `apply_code_action` runs any code action that server offers. A run over its files is verified by
//! the server's error diagnostics and the grammar's parse ([`verify`]).
//!
//! An extension the user opted in (`extensions enable-wasm <id>`) with an `extension.wasm` is asked
//! for a declared server's command when no `binary.path` is set ([`wasm`]); no other extension code
//! ever runs.
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
pub mod server;
pub mod settings;
pub mod support;
#[cfg(feature = "testkit")]
pub mod testkit;
pub mod verify;
pub mod wasm;

mod archive;
mod staging;

pub use dir::ExtensionsDir;
pub use error::ExtensionError;
pub use grammar::{GrammarLoadError, GrammarStore};
pub use index::{set_wasm_enabled, ExtensionSource, Index, IndexEntry};
pub use install::{
    install_from_path, install_from_registry, list, remove, InstallOutcome, InstalledExtension,
};
pub use language::{ExtensionLanguage, LanguageConfig};
pub use manifest::{ExtensionManifest, GrammarEntry, LanguageServerEntry, LibEntry, LibKind};
pub use outline::{Outline, OutlineEntry};
pub use registry::Registry;
pub use server::{client_capabilities, NoServerCommand, ServerChoice, ServerRefusal};
pub use settings::{BinarySettings, ExtensionSettings, ServerSettings};
pub use support::{
    load_installed, ExtensionSupport, InstalledExtensions, InstalledLanguages, OutlineBackend,
};
pub use verify::ExtensionVerify;
pub use wasm::{
    choose_server, WasmCommand, WasmCommandError, WasmExtension, WasmHost, WasmWorktree,
};
