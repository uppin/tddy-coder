use crate::ToolOutcome;
use std::path::Path;

/// Dispatch one of the five language-agnostic LSP tools to the registered
/// [`tddy_core::toolcall::lsp::LspExecutor`]. The executor's methods are synchronous and
/// block on an internal runtime, so they are run on a blocking thread via `spawn_blocking`.
pub(crate) async fn tool_lsp(
    worktree_root: &Path,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolOutcome {
    let Some(executor) = tddy_core::toolcall::lsp::lsp_executor() else {
        return ToolOutcome::err(format!("{tool_name}: no language server available"));
    };
    let query = match parse_lsp_query(args) {
        Ok(q) => q,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: {e}")),
    };
    let root = worktree_root.to_path_buf();
    let name = tool_name.to_string();
    let result = tokio::task::spawn_blocking(move || match name.as_str() {
        "LspDiagnostics" => executor.diagnostics(&root, &query),
        "LspDefinition" => executor.definition(&root, &query),
        "LspReferences" => executor.references(&root, &query),
        "LspHover" => executor.hover(&root, &query),
        "LspSymbols" => executor.symbols(&root, &query),
        other => Err(format!("unknown lsp tool: {other}")),
    })
    .await;
    match result {
        Ok(Ok(value)) => ToolOutcome::ok(value.to_string()),
        Ok(Err(e)) => ToolOutcome::err(format!("{tool_name}: {e}")),
        Err(e) => ToolOutcome::err(format!("{tool_name}: {e}")),
    }
}

fn parse_lsp_query(args: &serde_json::Value) -> Result<tddy_core::toolcall::lsp::LspQuery, String> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or("missing 'target' argument")?
        .to_string();
    let file = args
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = args.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let symbol_query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(tddy_core::toolcall::lsp::LspQuery {
        target,
        file,
        line,
        character,
        symbol_query,
    })
}

/// Workspace-level diagnostics. Routes to the registered language-server executor when one
/// is available for the repo (`{"lints":[…]}`); otherwise returns the no-linter stub.
pub(crate) async fn tool_read_lints(worktree_root: &Path) -> ToolOutcome {
    if let Some(executor) = tddy_core::toolcall::lsp::lsp_executor() {
        if executor.is_available(worktree_root) {
            let root = worktree_root.to_path_buf();
            let result =
                tokio::task::spawn_blocking(move || executor.workspace_diagnostics(&root)).await;
            return match result {
                Ok(Ok(value)) => ToolOutcome::ok(value.to_string()),
                Ok(Err(e)) => ToolOutcome::err(format!("ReadLints: {e}")),
                Err(e) => ToolOutcome::err(format!("ReadLints: {e}")),
            };
        }
    }
    ToolOutcome::ok(
        serde_json::json!({
            "lints": [],
            "note": "ReadLints: no linter configured in this environment"
        })
        .to_string(),
    )
}

/// Semantic (embedding-backed) search over the session's index. There is deliberately no lexical
/// fallback: SemanticSearch is only meaningful with a populated semantic index, and silently
/// degrading to ripgrep would return lexically-similar-but-semantically-wrong results the caller
/// cannot distinguish from real hits. The index location is passed via `TDDY_SEMANTIC_INDEX_DB`
/// (see docs/ft/coder/semantic-index.md); its absence — or a path that does not exist — is an
/// error, not a cue to search some other way.
pub(crate) async fn tool_semantic_search(
    _root: &Path,
    args: &serde_json::Value,
    extra_env: &[(String, String)],
) -> ToolOutcome {
    if args.get("query").and_then(|v| v.as_str()).is_none() {
        return ToolOutcome::err("SemanticSearch: missing 'query' argument");
    }

    let db_path = extra_env
        .iter()
        .find(|(k, _)| k == "TDDY_SEMANTIC_INDEX_DB")
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.trim().is_empty());

    let db_path =
        match db_path {
            Some(path) => path,
            None => return ToolOutcome::err(
                "SemanticSearch: no semantic index for this session (TDDY_SEMANTIC_INDEX_DB unset)",
            ),
        };

    if !Path::new(db_path).exists() {
        return ToolOutcome::err(format!(
            "SemanticSearch: no semantic index for this session (index DB not found at {db_path})"
        ));
    }

    // TODO: query the populated index via the production embedder. The embedder that turns the
    // query into a vector for nearest-neighbour lookup is not wired into the engine yet; until it
    // is, an existing index cannot be queried. See docs/ft/coder/semantic-index.md.
    ToolOutcome::err("SemanticSearch: index query not yet wired")
}
