//! A file's symbols, a root's matching symbols and a file's diagnostics, answered by the root's warm
//! language server.
//!
//! The navigation module's shape, for the two queries an agent's `LspSymbols` and `LspDiagnostics`
//! tools need: one `tddy_lsp::LspClient` query (`symbols` / `workspace_symbols`, `diagnostics`) on
//! the client `WorkspaceIndex::client_for` hands out for the request's root, with positions
//! translated to the service's one-based byte coordinates and locations made relative to the root.

use std::path::Path;

use tddy_lsp::client::Range;
use tddy_lsp::SymbolInfo;
use tddy_rpc::Status;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::navigation::{code_location, served_source, synced_document, wire_position};
use crate::proto::code_index::{
    CodeDiagnostic, CodeSymbol, DiagnosticsRequest, DiagnosticsResponse, SourceRange,
    SymbolsRequest, SymbolsResponse,
};
use crate::status::status_of_lsp;

/// The symbols of the request's file, or the root's symbols matching its `query`.
pub(crate) async fn serve_symbols(
    index: &WorkspaceIndex,
    request: SymbolsRequest,
) -> Result<SymbolsResponse, Status> {
    let (activity, root) = Activity::arrived("symbols", index, &request.workspace_root).await?;
    let outcome = symbols_of(index, &root, &request.file, request.query.as_deref()).await;
    activity.recorded(outcome)
}

/// What the language server reports wrong with the request's file.
pub(crate) async fn serve_diagnostics(
    index: &WorkspaceIndex,
    request: DiagnosticsRequest,
) -> Result<DiagnosticsResponse, Status> {
    let (activity, root) = Activity::arrived("diagnostics", index, &request.workspace_root).await?;
    let outcome = diagnostics_of(index, &root, &request.file).await;
    activity.recorded(outcome)
}

async fn symbols_of(
    index: &WorkspaceIndex,
    root: &Path,
    file: &str,
    query: Option<&str>,
) -> Result<SymbolsResponse, Status> {
    let answered = match query {
        Some(query) => index
            .client_for(root)
            .await?
            .workspace_symbols(query)
            .await
            .map_err(|failure| status_of_lsp(&failure))?,
        None => {
            let path = served_source(root, file)?;
            let (client, uri) = synced_document(index, root, &path, file).await?;
            client
                .symbols(&uri)
                .await
                .map_err(|failure| status_of_lsp(&failure))?
        }
    };
    let symbols = answered
        .iter()
        .map(|symbol| code_symbol(root, symbol))
        .collect::<Result<_, _>>()?;
    Ok(SymbolsResponse { symbols })
}

async fn diagnostics_of(
    index: &WorkspaceIndex,
    root: &Path,
    file: &str,
) -> Result<DiagnosticsResponse, Status> {
    let path = served_source(root, file)?;
    let (client, uri) = synced_document(index, root, &path, file).await?;
    let answered = client
        .diagnostics(&uri)
        .await
        .map_err(|failure| status_of_lsp(&failure))?;
    let diagnostics = answered
        .into_iter()
        .map(|diagnostic| CodeDiagnostic {
            range: Some(wire_range(diagnostic.range)),
            severity: u32::from(diagnostic.severity),
            message: diagnostic.message,
            source: diagnostic.source,
        })
        .collect();
    Ok(DiagnosticsResponse { diagnostics })
}

fn code_symbol(root: &Path, symbol: &SymbolInfo) -> Result<CodeSymbol, Status> {
    Ok(CodeSymbol {
        name: symbol.name.clone(),
        kind: u32::from(symbol.kind),
        location: Some(code_location(root, &symbol.location)?),
        container: symbol.container.clone(),
    })
}

fn wire_range(range: Range) -> SourceRange {
    SourceRange {
        start: Some(wire_position(range.start)),
        end: Some(wire_position(range.end)),
    }
}
