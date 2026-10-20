//! A language whose server is any LSP, not rust-analyzer: `rename_symbol` through
//! `textDocument/rename`, every item anchor through the outline backend it is built over
//! (`#reshape` extension-language-server, R6).
//!
//! Nothing here knows a language. The support that builds it — an installed extension's — hands in
//! the backend that already answers its item anchors, and the server it started from the launch the
//! user configured. A support whose server has no configured command builds it
//! [`ServerBackend::without_command`], so a rename is refused in the support's words before any
//! process starts (R3) rather than as an operation the language does not have.

use std::sync::Arc;

use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use crate::item_anchor::{ItemAtResolver, ItemResolver};
use crate::language::LiveServer;
use crate::plan::{RefactorKind, RefactorOp};
use crate::registry::{LanguageBackend, Workspace};
use crate::{Resolution, Result};
use tddy_lsp::LanguageId;

/// A language's backend over a generic language server, composed with its outline backend.
pub struct ServerBackend {
    /// Answers the item anchors — the language's id, the extensions it claims, `item` / `items` /
    /// `--at` — exactly as it does with no server.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-language-server): delegated to once implemented"
    )]
    outline: Box<dyn LanguageBackend>,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-language-server): read by resolve once implemented"
    )]
    server: Server,
}

/// What a rename is put to.
#[allow(
    dead_code,
    reason = "TODO(reshape-extension-language-server): read by resolve once implemented"
)]
enum Server {
    /// The language's running server.
    Live {
        client: Arc<LspClient>,
        /// What ends a wait on the server; the caller's.
        cancel: CancellationToken,
        /// The LSP `languageId` a document is opened as — the launch's.
        language_id: String,
    },
    /// The server needs a command nobody configured: every server operation is refused with this.
    NoCommand { reason: String },
}

impl ServerBackend {
    /// The backend over `server`, a running server of the language `outline` answers anchors for,
    /// opening documents as `language_id`.
    pub fn over(
        outline: Box<dyn LanguageBackend>,
        server: &LiveServer<'_>,
        language_id: impl Into<String>,
    ) -> Self {
        let _ = (outline, server, language_id.into());
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::over")
    }

    /// The backend of a language whose server has no configured command: item anchors still
    /// answer, and every server operation is [`crate::RestructureError::ServerNotConfigured`]
    /// carrying `reason` (R3).
    pub fn without_command(outline: Box<dyn LanguageBackend>, reason: impl Into<String>) -> Self {
        let _ = (outline, reason.into());
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::without_command")
    }
}

impl LanguageBackend for ServerBackend {
    /// The outline backend's.
    fn language(&self) -> &LanguageId {
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::language")
    }

    /// The outline backend's.
    fn handles_extension(&self, extension: &str) -> bool {
        let _ = extension;
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::handles_extension")
    }

    /// `rename_symbol` only; code actions are `extension-code-actions`' (K23).
    fn supports(&self, kind: RefactorKind) -> bool {
        let _ = kind;
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::supports")
    }

    /// R6: the anchor resolves to an item through the outline backend; the document is opened as
    /// the launch's `languageId`, `textDocument/rename` is sent at the item's name with
    /// `op.name`, and the returned `WorkspaceEdit` — paths relative to the root — is the
    /// resolution. Without a command: [`crate::RestructureError::ServerNotConfigured`].
    fn resolve(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> {
        let _ = (op, workspace);
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::resolve")
    }

    /// The outline backend's.
    fn item_resolver(&mut self) -> Option<&mut dyn ItemResolver> {
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::item_resolver")
    }

    /// The outline backend's.
    fn item_locator(&mut self) -> Option<&mut dyn ItemAtResolver> {
        // TODO(reshape-extension-language-server): implement
        todo!("ServerBackend::item_locator")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;
    use crate::RestructureError;
    use std::path::Path;

    /// An outline backend that claims `.toml`, supports nothing and answers no anchors.
    struct AnOutlineBackend {
        id: LanguageId,
    }

    impl LanguageBackend for AnOutlineBackend {
        fn language(&self) -> &LanguageId {
            &self.id
        }

        fn handles_extension(&self, extension: &str) -> bool {
            extension == "toml"
        }

        fn supports(&self, _kind: RefactorKind) -> bool {
            false
        }

        fn resolve(&mut self, _op: &RefactorOp, _workspace: &Workspace<'_>) -> Result<Resolution> {
            unreachable!("an outline backend resolves nothing")
        }
    }

    fn an_outline_backend_for(id: &str) -> Box<dyn LanguageBackend> {
        Box::new(AnOutlineBackend {
            id: LanguageId::new(id),
        })
    }

    const NO_COMMAND: &str = "language server fake-ls (extension demo-toml) has no command: \
                              set lsp.fake-ls.binary.path in /data/extensions/settings.json";

    fn a_rename_of_port() -> RefactorOp {
        serde_json::from_str(
            r#"{"op":"rename_symbol","anchor":{"kind":"range","file":"config/app.toml","start":{"line":3,"col":1},"end":{"line":3,"col":5}},"name":"listen_port"}"#,
        )
        .expect("a rename parses")
    }

    #[test]
    fn rename_is_the_only_operation_it_supports() {
        // Given a backend over an outline, its server unconfigured
        let backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);

        // When it is asked which operations it performs
        let supported = [
            RefactorKind::RenameSymbol,
            RefactorKind::ExtractMethod,
            RefactorKind::MoveItem,
        ]
        .map(|kind| backend.supports(kind));

        // Then rename alone
        assert_eq!(supported, [true, false, false]);
    }

    #[test]
    fn it_is_the_outline_backends_language() {
        // Given a backend over the demo-toml outline
        let backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);

        // When it is routed to
        let routed = (
            backend.language().to_string(),
            backend.handles_extension("toml"),
            backend.handles_extension("rs"),
        );

        // Then it answers for the outline's language and files
        assert_eq!(routed, ("demo-toml".to_string(), true, false));
    }

    #[test]
    fn without_a_command_a_rename_is_refused_with_the_supports_reason() {
        // Given a backend whose server has no configured command
        let mut backend =
            ServerBackend::without_command(an_outline_backend_for("demo-toml"), NO_COMMAND);
        let overlay = Overlay::new();

        // When a rename is resolved
        let refused = backend.resolve(
            &a_rename_of_port(),
            &Workspace {
                root: Path::new("/workspace"),
                overlay: &overlay,
            },
        );

        // Then it is refused in the support's words, naming the language
        match refused {
            Err(RestructureError::ServerNotConfigured { language, reason }) => {
                assert_eq!(
                    (language.as_str(), reason.as_str()),
                    ("demo-toml", NO_COMMAND)
                )
            }
            other => panic!("expected ServerNotConfigured, got {other:?}"),
        }
    }
}
