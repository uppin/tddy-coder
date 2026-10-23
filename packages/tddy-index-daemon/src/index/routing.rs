use crate::readiness::Readiness;
use std::sync::Arc;
use tddy_lsp::client::LspClient;
use tddy_lsp::{LanguageId, LaunchSpec};

/// What serves one file of a root (`#reshape` extension-language-server, R7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    /// The built-in Rust server.
    Rust,
    /// An extension language's server, launched as `launch` says.
    Extension {
        language: LanguageId,
        launch: Box<LaunchSpec>,
    },
}

/// A root's server for one file, and how to talk to it about that file.
pub(crate) struct ServedFile {
    pub(crate) client: Arc<LspClient>,
    /// The LSP `languageId` a document of the file's language is announced under.
    pub(crate) language_id: String,
    /// How this server's answers are read into an index state.
    pub(crate) readiness: Readiness,
}
