//! Rust as a [`LanguageSupport`]: the built-in every other support is registered after.
//!
//! It wraps what was wired in by hand before: `.rs` files, rust-analyzer launched with
//! [`client_capabilities`] and [`server_settings`], the Cargo workspace-root rule, and
//! [`RustBackend`] built over the shared client or, for the static tier, over nothing.
//!
//! [`client_capabilities`]: super::client_capabilities
//! [`server_settings`]: super::server_settings
//! [`RustBackend`]: super::RustBackend

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_lsp::registry::workspace_root_within;
use tddy_lsp::{LanguageId, LaunchSpec, LspError};

use super::RustBackend;
use crate::language::{LanguageSupport, ServerHandle};
use crate::registry::LanguageBackend;

/// The placeholders the static tier's backend is built with, as the static tier has always built
/// it. A static backend has no server, and one that reaches `start` reads its toolchain from
/// [`STATIC_PLACEHOLDER_HOME`] and is refused there unless that directory holds a rustup settings
/// file, before anything is spawned.
const STATIC_PLACEHOLDER_BINARY: &str = "/usr/bin/rust-analyzer";
const STATIC_PLACEHOLDER_HOME: &str = "/tmp";

/// The built-in Rust support.
#[derive(Debug, Clone)]
pub struct RustSupport {
    program: String,
}

impl RustSupport {
    /// Rust, launching `rust-analyzer` from `PATH`.
    pub fn new() -> Self {
        Self::launching("rust-analyzer")
    }

    /// Rust, launching the rust-analyzer at `program`.
    pub fn launching(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// The launch the built-in Rust support runs: rust-analyzer with the handshake the restructure
    /// backend needs.
    ///
    /// A server told nothing answers accordingly: it returns no code actions at all, which reads as
    /// a range that supports no refactoring, and it counts positions in utf-16 code units while this
    /// client counts bytes. Both are settled by the handshake, so the handshake is what this carries.
    /// The documents it opens are announced as the launch's own `language_id`.
    fn the_launch(&self) -> LaunchSpec {
        LaunchSpec::new(self.program.clone())
            .with_capabilities(super::client_capabilities())
            .with_initialization_options(super::server_settings())
            .with_language_id(self.id().as_str())
    }
}

impl Default for RustSupport {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageSupport for RustSupport {
    fn id(&self) -> &LanguageId {
        &super::RUST_LANGUAGE
    }

    /// A `.rs` file, the rule the Rust backend has always applied to an extension.
    fn claims(&self, path: &Path) -> bool {
        path.extension() == Some(OsStr::new("rs"))
    }

    fn launch(&self) -> Option<LaunchSpec> {
        Some(self.the_launch())
    }

    /// The Cargo workspace root `file` belongs to, searched no further out than `root`.
    ///
    /// A relative `file` is read against `root`. A file outside `root`, or one with no directory to
    /// search from, is an error rather than a root chosen for it.
    fn workspace_root(&self, file: &Path, root: &Path) -> Result<PathBuf, LspError> {
        let absolute = root.join(file);
        let Some(directory) = absolute.parent() else {
            return Err(LspError::Io(format!(
                "{} has no directory to search for a workspace root from",
                absolute.display()
            )));
        };
        workspace_root_within(directory, root)
    }

    /// The backend over a live server's shared client, or over no server for the static tier.
    ///
    /// `_root` is not stored. A bridged client was initialized against the root its registry key
    /// named, and a self-spawned server takes its root from the operation that starts it. Storing
    /// `root` on a static backend would make its workspace root answer, and an operation that asks
    /// for that root would then start a server it was never given.
    fn backend(&self, server: ServerHandle<'_>, _root: &Path) -> Box<dyn LanguageBackend> {
        let language_id = self.the_launch().language_id;
        match server.server() {
            Some(live) => {
                let mut rust = RustBackend::from_lsp_client(
                    Arc::clone(live.client),
                    Some(live.cancel.clone()),
                    Arc::clone(live.progress),
                )
                .with_wait_heartbeat(live.wait_heartbeat)
                .with_document_language(language_id);
                if let Some(trace) = live.trace {
                    rust = rust.with_trace(trace);
                }
                Box::new(rust)
            }
            None => Box::new(
                RustBackend::new(
                    STATIC_PLACEHOLDER_BINARY,
                    STATIC_PLACEHOLDER_HOME,
                    STATIC_PLACEHOLDER_HOME,
                )
                .with_document_language(language_id),
            ),
        }
    }
}
