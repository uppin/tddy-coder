//! Rust as a [`LanguageSupport`]: the built-in every other support is registered after.
//!
//! It wraps what was wired in by hand before: `.rs` files, rust-analyzer launched with
//! [`client_capabilities`] and [`server_settings`], the Cargo workspace-root rule, and
//! [`RustBackend`] built over the shared client or, for the static tier, over nothing.
//!
//! [`client_capabilities`]: super::client_capabilities
//! [`server_settings`]: super::server_settings
//! [`RustBackend`]: super::RustBackend

use std::path::{Path, PathBuf};

use tddy_lsp::{LanguageId, LaunchSpec};

use crate::language::{LanguageSupport, ServerHandle};
use crate::registry::LanguageBackend;

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

    fn claims(&self, path: &Path) -> bool {
        let _ = path;
        // TODO(reshape-language-support): implement
        todo!("RustSupport::claims")
    }

    fn launch(&self) -> Option<LaunchSpec> {
        let _ = &self.program;
        // TODO(reshape-language-support): implement
        todo!("RustSupport::launch")
    }

    fn workspace_root(&self, file: &Path, root: &Path) -> PathBuf {
        let _ = (file, root);
        // TODO(reshape-language-support): implement
        todo!("RustSupport::workspace_root")
    }

    fn backend(&self, server: ServerHandle<'_>, root: &Path) -> Box<dyn LanguageBackend> {
        let _ = (server, root);
        // TODO(reshape-language-support): implement
        todo!("RustSupport::backend")
    }
}
