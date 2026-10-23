use crate::activity::Warmth;
use crate::index::RootState;
use crate::index::WorkspaceIndex;
use crate::index::REQUEST_BOUND_ABOVE_ANY_COLD_INDEX;
use crate::readiness::Readiness;
use crate::status::status_of_lsp;
use std::path::Path;
use std::sync::Arc;
use tddy_lsp::client::LspClient;
use tddy_lsp::registry::LspKey;
use tddy_lsp::{LanguageId, LaunchSpec};
use tddy_rpc::Status;

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

impl WorkspaceIndex {
    /// What serves `path`, a file of `root` the request named as `file`.
    ///
    /// Rust's `.rs` files are the built-in server's, whatever an extension also claims. Any other
    /// file goes to the support that claims it — an installed extension's — when that support
    /// launches a server; an outline-only language, or a file nothing claims, is refused (R7).
    pub(crate) fn route_of(&self, path: &Path, file: &str) -> Result<Route, Status> {
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            return Ok(Route::Rust);
        }
        let Some(support) = self.supports.support_of(path) else {
            return Err(Status::invalid_argument(format!(
                "`{file}` is not a `.rs` file, and no installed language claims it; this index \
                 serves rust and the installed languages that have a language server"
            )));
        };
        let language = support.id().clone();
        let Some(mut launch) = support.launch() else {
            return Err(Status::invalid_argument(format!(
                "`{file}` is a {language} file, and {language} has no language server configured \
                 to answer for it"
            )));
        };
        if launch.language_id.is_empty() {
            launch.language_id = language.as_str().to_string();
        }
        Ok(Route::Extension {
            language,
            launch: Box::new(launch),
        })
    }

    /// The root's warm server for `route`, spawning one if this process holds none yet.
    ///
    /// Rust's is [`Self::client_for`]. An extension language's is handed out as it is, with no
    /// tree changes told to it: only Rust sources are tracked, and a document the server is asked
    /// about is announced afresh with its current text on every request.
    pub(crate) async fn client_for_route(
        &self,
        root: &Path,
        route: &Route,
    ) -> Result<ServedFile, Status> {
        match route {
            Route::Rust => Ok(ServedFile {
                client: self.client_for(root).await?,
                language_id: "rust".to_string(),
                readiness: Readiness::ServerStatus,
            }),
            Route::Extension { language, launch } => {
                let key = LspKey {
                    root: root.to_path_buf(),
                    language: language.clone(),
                };
                let service = self.servers.get_or_spawn(key).await.map_err(|failure| {
                    let refusal = status_of_lsp(&failure);
                    self.signals.failed(root, refusal.message());
                    refusal
                })?;
                service
                    .client
                    .set_request_timeout(REQUEST_BOUND_ABOVE_ANY_COLD_INDEX);
                let readiness = Readiness::of_launch(launch);
                self.record_use(root, &service.client, readiness, None)
                    .await;
                self.roots
                    .lock()
                    .await
                    .entry(root.to_path_buf())
                    .or_insert_with(RootState::new)
                    .languages
                    .insert(language.clone());
                Ok(ServedFile {
                    client: Arc::clone(&service.client),
                    language_id: launch.language_id.clone(),
                    readiness,
                })
            }
        }
    }

    /// Whether the registry holds a server for `root`, of Rust or of a language one was reached
    /// for at it.
    pub(super) async fn holds_a_server_for(&self, root: &Path) -> bool {
        let mut languages = vec![LanguageId::RUST];
        if let Some(state) = self.roots.lock().await.get(root) {
            languages.extend(state.languages.iter().cloned());
        }
        for language in languages {
            let key = LspKey {
                root: root.to_path_buf(),
                language,
            };
            if self.servers.get(&key).await.is_some() {
                return true;
            }
        }
        false
    }

    /// Whether this process already holds an index for `root`.
    ///
    /// Asked *before* the work starts, because it is what an operator needs to read the request
    /// that follows: the same `check` answers in milliseconds against a warm root and waits six to
    /// ten minutes for a cold one. Answered against the registry rather than against this host's
    /// own map, for the reason [`Self::warm_workspaces`] gives — a root whose server has been
    /// reaped is not warm, whatever this host once asked for.
    pub(crate) async fn warmth_of(&self, root: &Path) -> Warmth {
        if self.holds_a_server_for(root).await {
            Warmth::AlreadyWarm
        } else {
            Warmth::NotYetIndexed
        }
    }
}
