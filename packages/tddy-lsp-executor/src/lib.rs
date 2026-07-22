//! Concrete [`LspExecutor`] that binds `tddy-build` target discovery to the `tddy-lsp`
//! registry. It is registered process-globally by the daemon and the sandbox-app (the
//! hosts that run `tddy-tool-engine`), so relayed `Lsp*` tool calls resolve to a real,
//! reused language server. `tddy-core` deliberately owns only the trait; this crate owns
//! the impl (mirroring how `tddy-coder` owns the concrete `BuildExecutor`).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_build::service::{build_list_json, BuildListQuery};
use tddy_core::toolcall::lsp::{register_lsp_executor, LspExecutor, LspQuery};
use tddy_lsp::registry::workspace_root_for;
use tddy_lsp::{
    language_for_target_type, Diagnostic, DocumentSource, Language, Location, LspAllowList, LspKey,
    LspRegistry, LspService, Position, Range, SymbolInfo,
};
use tddy_task::TaskRegistry;

/// Register a process-global executor over `allow`, sharing `task_registry` (so LSP
/// servers appear as ordinary tasks) and reaping servers idle past `idle_timeout`.
/// Returns the underlying [`LspRegistry`] so the caller can drive an idle-reaper loop.
pub fn register(
    task_registry: TaskRegistry,
    allow: LspAllowList,
    idle_timeout: Duration,
) -> LspRegistry {
    let executor = TddyLspExecutor::new(allow, task_registry, idle_timeout);
    let registry = executor.registry();
    register_lsp_executor(Arc::new(executor));
    registry
}

/// Resolves a build target to its language server and serves LSP queries against a shared,
/// reused server per (workspace root, language).
pub struct TddyLspExecutor {
    allow: LspAllowList,
    registry: LspRegistry,
}

impl TddyLspExecutor {
    /// Build an executor over `allow`, spawning servers on `task_registry`.
    pub fn new(allow: LspAllowList, task_registry: TaskRegistry, idle_timeout: Duration) -> Self {
        let registry = LspRegistry::new(allow.clone(), task_registry, idle_timeout);
        Self { allow, registry }
    }

    /// The underlying registry (e.g. for an idle-reaper loop).
    pub fn registry(&self) -> LspRegistry {
        self.registry.clone()
    }

    /// The allowed language for a target id, resolved from its `BUILD.yaml` `config.type`.
    fn language_for_target(&self, repo_dir: &Path, target: &str) -> Option<Language> {
        let list = build_list_json(repo_dir, &BuildListQuery::default()).ok()?;
        let targets = list.get("targets")?.as_array()?;
        let type_name = targets
            .iter()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(target))
            .and_then(|t| t.get("type").and_then(Value::as_str))?;
        language_for_target_type(type_name)
    }

    /// The first allowed language found among the repo's build targets, if any. Backs both
    /// `is_available` and workspace-level diagnostics.
    fn first_available_language(&self, repo_dir: &Path) -> Option<Language> {
        let list = build_list_json(repo_dir, &BuildListQuery::default()).ok()?;
        let targets = list.get("targets")?.as_array()?;
        targets.iter().find_map(|t| {
            t.get("type")
                .and_then(Value::as_str)
                .and_then(language_for_target_type)
                .filter(|lang| self.allow.is_allowed(*lang))
        })
    }

    /// Resolve a target to its language + reuse key, rejecting disallowed languages.
    fn resolve(&self, repo_dir: &Path, target: &str) -> Result<(Language, LspKey), String> {
        let language = self
            .language_for_target(repo_dir, target)
            .ok_or_else(|| format!("no language server for target '{target}'"))?;
        if !self.allow.is_allowed(language) {
            return Err(format!("language '{}' is not allowed", language.id()));
        }
        let key = LspKey {
            root: workspace_root_for(repo_dir),
            language,
        };
        Ok((language, key))
    }

    /// Get-or-spawn the server and open the query's document so the server indexes it.
    async fn service_for(
        &self,
        repo_dir: &Path,
        query: &LspQuery,
    ) -> Result<(Arc<LspService>, String), String> {
        let (language, key) = self.resolve(repo_dir, &query.target)?;
        let uri = file_uri(repo_dir, &query.file);
        let text = std::fs::read_to_string(repo_dir.join(&query.file)).unwrap_or_default();
        let srcs = [DocumentSource {
            uri: uri.clone(),
            language_id: language.id().to_string(),
            text,
        }];
        let service = self
            .registry
            .bind_target(key, &srcs)
            .await
            .map_err(|e| e.to_string())?;
        Ok((service, uri))
    }
}

/// Run an async body from a synchronous trait method. Callers invoke the executor from a
/// blocking context (`spawn_blocking`), where the runtime handle is available and blocking
/// is permitted — the same convention the `BuildExecutor` listener uses.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Handle::current().block_on(fut)
}

impl LspExecutor for TddyLspExecutor {
    fn is_available(&self, repo_dir: &Path) -> bool {
        self.first_available_language(repo_dir).is_some()
    }

    fn diagnostics(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        block_on(async {
            let (service, uri) = self.service_for(repo_dir, query).await?;
            let diagnostics = service
                .client
                .diagnostics(&uri)
                .await
                .map_err(|e| e.to_string())?;
            Ok(
                json!({ "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>() }),
            )
        })
    }

    fn definition(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        block_on(async {
            let (service, uri) = self.service_for(repo_dir, query).await?;
            let locations = service
                .client
                .definition(&uri, position(query))
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "locations": locations.iter().map(location_json).collect::<Vec<_>>() }))
        })
    }

    fn references(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        block_on(async {
            let (service, uri) = self.service_for(repo_dir, query).await?;
            let locations = service
                .client
                .references(&uri, position(query))
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "references": locations.iter().map(location_json).collect::<Vec<_>>() }))
        })
    }

    fn hover(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        block_on(async {
            let (service, uri) = self.service_for(repo_dir, query).await?;
            let hover = service
                .client
                .hover(&uri, position(query))
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "hover": hover }))
        })
    }

    fn symbols(&self, repo_dir: &Path, query: &LspQuery) -> Result<Value, String> {
        block_on(async {
            let (service, uri) = self.service_for(repo_dir, query).await?;
            let symbols = if let Some(q) = &query.symbol_query {
                service.client.workspace_symbols(q).await
            } else {
                service.client.symbols(&uri).await
            }
            .map_err(|e| e.to_string())?;
            Ok(json!({ "symbols": symbols.iter().map(symbol_json).collect::<Vec<_>>() }))
        })
    }

    fn workspace_diagnostics(&self, repo_dir: &Path) -> Result<Value, String> {
        let language = self
            .first_available_language(repo_dir)
            .ok_or_else(|| "no language server available for this workspace".to_string())?;
        let key = LspKey {
            root: workspace_root_for(repo_dir),
            language,
        };
        block_on(async {
            let service = self
                .registry
                .get_or_spawn(key)
                .await
                .map_err(|e| e.to_string())?;
            let groups = service
                .client
                .workspace_diagnostics()
                .await
                .map_err(|e| e.to_string())?;
            let lints: Vec<Value> = groups
                .iter()
                .flat_map(|(uri, diags)| diags.iter().map(move |d| lint_json(uri, d)))
                .collect();
            Ok(json!({ "lints": lints }))
        })
    }
}

fn position(query: &LspQuery) -> Position {
    Position::at(query.line, query.character)
}

fn file_uri(repo_dir: &Path, file: &str) -> String {
    format!("file://{}", repo_dir.join(file).display())
}

fn range_json(r: &Range) -> Value {
    json!({
        "start": { "line": r.start.line, "character": r.start.character },
        "end": { "line": r.end.line, "character": r.end.character },
    })
}

fn location_json(l: &Location) -> Value {
    json!({ "uri": l.uri, "range": range_json(&l.range) })
}

fn diagnostic_json(d: &Diagnostic) -> Value {
    json!({
        "range": range_json(&d.range),
        "severity": d.severity,
        "message": d.message,
        "source": d.source,
    })
}

/// A diagnostic tagged with its document uri, for the workspace-level `lints` array.
fn lint_json(uri: &str, d: &Diagnostic) -> Value {
    json!({
        "uri": uri,
        "range": range_json(&d.range),
        "severity": d.severity,
        "message": d.message,
        "source": d.source,
    })
}

fn symbol_json(s: &SymbolInfo) -> Value {
    json!({
        "name": s.name,
        "kind": s.kind,
        "location": location_json(&s.location),
        "container": s.container,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway repo dir holding a single `BUILD.yaml`, cleaned up on drop.
    struct TempRepo {
        dir: std::path::PathBuf,
    }

    impl TempRepo {
        fn with_build_yaml(body: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "tddy-lsp-executor-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("BUILD.yaml"), body).unwrap();
            Self { dir }
        }

        fn path(&self) -> &Path {
            &self.dir
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn rust_executor() -> TddyLspExecutor {
        TddyLspExecutor::new(
            LspAllowList::rust_only(),
            TaskRegistry::new(),
            Duration::from_secs(60),
        )
    }

    #[test]
    fn a_rust_repo_reports_an_available_language_server() {
        // Given a repo whose only target is a Rust binary
        let repo = TempRepo::with_build_yaml(
            "schema_version: 1\ntargets:\n  - id: \"app:bin\"\n    name: bin\n    config:\n      type: rust_binary\n",
        );
        let executor = rust_executor();

        // When we ask whether a language server is available
        let available = executor.is_available(repo.path());

        // Then it is
        assert!(
            available,
            "expected a Rust repo to have an available language server"
        );
    }

    #[test]
    fn a_repo_with_no_allowed_target_type_reports_no_language_server() {
        // Given a repo whose only target is a non-Rust type
        let repo = TempRepo::with_build_yaml(
            "schema_version: 1\ntargets:\n  - id: \"img:app\"\n    name: app\n    config:\n      type: docker\n",
        );
        let executor = rust_executor();

        // When we ask whether a language server is available
        let available = executor.is_available(repo.path());

        // Then there is none
        assert!(
            !available,
            "expected no language server for a non-Rust repo"
        );
    }
}
