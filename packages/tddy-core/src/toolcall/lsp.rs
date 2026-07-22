//! Language-server extension point.
//!
//! `tddy-core` deliberately has **no** dependency on `tddy-lsp` or `tddy-build`. It defines
//! the [`LspExecutor`] trait and a process-global registry; the binary that owns a session
//! (`tddy-coder`, or the daemon) registers a concrete executor before serving tool calls, so
//! relayed `Lsp*` requests are answered co-located with the worktree. This mirrors
//! [`super::build`] exactly.

use std::path::Path;
use std::sync::{Arc, OnceLock};

/// A position-based query for definition / references / hover / symbols.
#[derive(Debug, Clone)]
pub struct LspQuery {
    /// Build target id the query is scoped to, e.g. `"packages/foo:binary"`.
    pub target: String,
    /// Repo-relative file path.
    pub file: String,
    /// Zero-based line.
    pub line: u32,
    /// Zero-based character.
    pub character: u32,
    /// Query string for workspace-symbol lookups (ignored by the position ops).
    pub symbol_query: Option<String>,
}

/// Serves language-server queries against a repository. Implemented in the binaries on top
/// of `tddy-lsp`. Methods are synchronous and return the language-agnostic JSON payload the
/// MCP tools relay verbatim; implementations that need an async runtime block internally
/// (callers invoke them off the async executor via `spawn_blocking`).
pub trait LspExecutor: Send + Sync {
    /// Whether any language server can be served for this repo (gates MCP tool exposure).
    fn is_available(&self, repo_dir: &Path) -> bool;

    /// Diagnostics for a query's document.
    fn diagnostics(&self, repo_dir: &Path, query: &LspQuery) -> Result<serde_json::Value, String>;

    /// Go-to-definition for a query.
    fn definition(&self, repo_dir: &Path, query: &LspQuery) -> Result<serde_json::Value, String>;

    /// Find-references for a query.
    fn references(&self, repo_dir: &Path, query: &LspQuery) -> Result<serde_json::Value, String>;

    /// Hover for a query.
    fn hover(&self, repo_dir: &Path, query: &LspQuery) -> Result<serde_json::Value, String>;

    /// Document/workspace symbols for a query.
    fn symbols(&self, repo_dir: &Path, query: &LspQuery) -> Result<serde_json::Value, String>;
}

static REGISTERED: OnceLock<Arc<dyn LspExecutor>> = OnceLock::new();

/// Register the process-wide LSP executor. The first registration wins.
pub fn register_lsp_executor(executor: Arc<dyn LspExecutor>) {
    let _ = REGISTERED.set(executor);
}

/// The registered executor, if any.
pub fn lsp_executor() -> Option<Arc<dyn LspExecutor>> {
    REGISTERED.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NamedExecutor(&'static str);

    impl NamedExecutor {
        fn payload(&self) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({ "name": self.0 }))
        }
    }

    impl LspExecutor for NamedExecutor {
        fn is_available(&self, _repo_dir: &Path) -> bool {
            true
        }
        fn diagnostics(
            &self,
            _repo_dir: &Path,
            _query: &LspQuery,
        ) -> Result<serde_json::Value, String> {
            self.payload()
        }
        fn definition(
            &self,
            _repo_dir: &Path,
            _query: &LspQuery,
        ) -> Result<serde_json::Value, String> {
            self.payload()
        }
        fn references(
            &self,
            _repo_dir: &Path,
            _query: &LspQuery,
        ) -> Result<serde_json::Value, String> {
            self.payload()
        }
        fn hover(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<serde_json::Value, String> {
            self.payload()
        }
        fn symbols(
            &self,
            _repo_dir: &Path,
            _query: &LspQuery,
        ) -> Result<serde_json::Value, String> {
            self.payload()
        }
    }

    // A single test drives the process-global registry: it must be empty first, and the
    // first registration must win. Splitting these would race on the shared `OnceLock`.
    #[test]
    fn the_first_registered_lsp_executor_wins_and_none_is_returned_before_registration() {
        // Given a process with no LSP executor registered yet
        assert!(lsp_executor().is_none());

        // When two executors are registered in turn
        register_lsp_executor(Arc::new(NamedExecutor("first")));
        register_lsp_executor(Arc::new(NamedExecutor("second")));

        // Then the first registration wins
        let executor = lsp_executor().expect("an executor to be registered");
        let query = LspQuery {
            target: "some:target".to_string(),
            file: "src/lib.rs".to_string(),
            line: 0,
            character: 0,
            symbol_query: None,
        };
        let payload = executor
            .diagnostics(Path::new("/repo"), &query)
            .expect("diagnostics");
        assert_eq!(payload, serde_json::json!({ "name": "first" }));
    }
}
