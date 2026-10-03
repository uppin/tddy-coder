//! An [`LspExecutor`] answered by the warm code-intelligence index the daemon manages, rather than
//! by a language server of its own.
//!
//! [`crate::TddyLspExecutor`] starts a rust-analyzer per `BUILD.yaml` target and pays a cold
//! crate-graph load for it. When the daemon manages an index (`index_daemon:` in its config), the
//! session's `Lsp*` tools ask that index instead — `code_index.CodeIndexService`'s `Definition`,
//! `References`, `Hover`, `Symbols` and `Diagnostics`, rooted at the session's worktree — so a
//! session and the web's code pane ask the same index and no second server is started.
//!
//! The worktree is bound **on the host**: the `repo_dir` every method receives is the worktree the
//! host resolved from the session (`tddy_tool_engine::execute_tool_with_env`'s `worktree_root`), and
//! [`bind_to_session_worktree`] refuses any queried file outside it before the index is asked. A
//! path the jail supplies is never used as the root.
//!
//! The tools' names, argument schemas and result JSON are [`crate::TddyLspExecutor`]'s, unchanged:
//! zero-based LSP positions in and out, `file://` URIs, the same top-level keys. The coordinate
//! translation to the index's one-based byte positions is this module's.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tddy_core::toolcall::lsp::{LspExecutor, LspQuery};

/// Where a channel to the warm index comes from — in production the daemon's
/// `IndexDaemonRegistry::connect`, which starts the index daemon on first use.
///
/// A port rather than the registry itself because the registry lives in `tddy-daemon`, which
/// depends on this crate.
#[async_trait::async_trait]
pub trait IndexChannel: Send + Sync {
    /// A gRPC channel to the index, starting it if nothing has yet.
    async fn connect(&self) -> Result<tonic::transport::Channel, String>;
}

/// Answers the `Lsp*` tools from the warm index reached through an [`IndexChannel`].
pub struct IndexLspExecutor {
    #[allow(dead_code)] // TODO(session-lsp-tools): dialled by every query.
    index: Arc<dyn IndexChannel>,
}

impl IndexLspExecutor {
    /// An executor asking the index `index` connects to.
    #[must_use]
    pub fn new(index: Arc<dyn IndexChannel>) -> Self {
        Self { index }
    }
}

/// Not served yet: every method answers this until the index is asked.
fn not_served_yet(tool: &str) -> String {
    format!("{tool} through the warm index is not served yet — TODO(session-lsp-tools)")
}

impl LspExecutor for IndexLspExecutor {
    fn is_available(&self, _repo_dir: &Path) -> bool {
        // TODO(session-lsp-tools): available when the worktree holds a Rust workspace root.
        false
    }

    fn diagnostics(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        // TODO(session-lsp-tools): bind the file, `code_index.Diagnostics`, render
        // `{"diagnostics":[…]}` in zero-based LSP coordinates.
        Err(not_served_yet("LspDiagnostics"))
    }

    fn definition(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        // TODO(session-lsp-tools): bind the file, `code_index.Definition` at the one-based byte
        // position, render `{"locations":[{uri, range}]}` in zero-based LSP coordinates.
        Err(not_served_yet("LspDefinition"))
    }

    fn references(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        // TODO(session-lsp-tools): as `definition`, through `code_index.References`, rendered as
        // `{"references":[…]}`.
        Err(not_served_yet("LspReferences"))
    }

    fn hover(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        // TODO(session-lsp-tools): as `definition`, through `code_index.Hover`, rendered as
        // `{"hover": markdown-or-null}`.
        Err(not_served_yet("LspHover"))
    }

    fn symbols(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        // TODO(session-lsp-tools): bind the file, `code_index.Symbols` (with `query` for a
        // workspace search), render `{"symbols":[{name, kind, location, container}]}`.
        Err(not_served_yet("LspSymbols"))
    }

    fn workspace_diagnostics(&self, _repo_dir: &Path) -> Result<Value, String> {
        // TODO(session-lsp-tools): `ReadLints` — the index has no workspace-wide diagnostics RPC;
        // decide between one and a refusal (see the changeset's Technical Debt).
        Err(not_served_yet("ReadLints"))
    }
}

/// The queried `file` as a path relative to the session's `worktree`, or a refusal when it names
/// anything outside it — an absolute path elsewhere, or a relative one that climbs out with `..`.
///
/// This is the host-side binding: `worktree` is what the host resolved from the session, and
/// nothing reaches the index for a file this refuses.
pub fn bind_to_session_worktree(_worktree: &Path, file: &str) -> Result<String, String> {
    // TODO(session-lsp-tools): resolve `file` against `worktree` lexically (the file may not exist
    // yet), refuse `{file} is outside the session's worktree` when the result leaves it, return
    // the relative path otherwise.
    Err(format!(
        "binding {file} to the session's worktree is not implemented — TODO(session-lsp-tools)"
    ))
}

/// The executor a host registers: the index-backed one when it manages an index, `existing`
/// otherwise. A deployment switch, not a fallback — with an index, `existing` is never asked.
#[must_use]
pub fn select_lsp_executor(
    index: Option<Arc<dyn IndexChannel>>,
    existing: Arc<dyn LspExecutor>,
) -> Arc<dyn LspExecutor> {
    match index {
        Some(index) => Arc::new(IndexLspExecutor::new(index)),
        None => existing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_file_inside_the_worktree_is_bound_to_it() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file inside it is bound
        let bound = bind_to_session_worktree(worktree, "src/lib.rs");

        // Then it is that file, relative to the worktree
        assert_eq!(bound, Ok("src/lib.rs".to_string()));
    }

    #[test]
    fn an_absolute_file_inside_the_worktree_is_made_relative_to_it() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file is named by its absolute path inside it
        let bound = bind_to_session_worktree(worktree, "/sessions/one/worktree/src/main.rs");

        // Then it is bound relative to the worktree
        assert_eq!(bound, Ok("src/main.rs".to_string()));
    }

    #[test]
    fn a_file_that_climbs_out_of_the_worktree_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file climbs into a neighbouring session's worktree
        let bound = bind_to_session_worktree(worktree, "../../two/worktree/src/lib.rs");

        // Then it is refused, naming the file
        assert_eq!(
            bound,
            Err("../../two/worktree/src/lib.rs is outside the session's worktree".to_string())
        );
    }

    #[test]
    fn an_absolute_file_in_another_worktree_is_refused() {
        // Given a session worktree
        let worktree = Path::new("/sessions/one/worktree");

        // When a file is named by its absolute path in another session's worktree
        let bound = bind_to_session_worktree(worktree, "/sessions/two/worktree/src/lib.rs");

        // Then it is refused, naming the file
        assert_eq!(
            bound,
            Err("/sessions/two/worktree/src/lib.rs is outside the session's worktree".to_string())
        );
    }
}
