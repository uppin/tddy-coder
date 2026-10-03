//! A file's symbols, a root's matching symbols and a file's diagnostics, answered by the root's warm
//! language server.
//!
//! The navigation module's shape, for the two queries an agent's `LspSymbols` and `LspDiagnostics`
//! tools need: one `tddy_lsp::LspClient` query (`symbols` / `workspace_symbols`, `diagnostics`) on
//! the client `WorkspaceIndex::client_for` hands out for the request's root, with positions
//! translated to the service's one-based byte coordinates and locations made relative to the root.

use tddy_rpc::Status;

use crate::index::WorkspaceIndex;
use crate::proto::code_index::{
    DiagnosticsRequest, DiagnosticsResponse, SymbolsRequest, SymbolsResponse,
};

/// The symbols of the request's file, or the root's symbols matching its `query`.
pub(crate) async fn serve_symbols(
    _index: &WorkspaceIndex,
    _request: SymbolsRequest,
) -> Result<SymbolsResponse, Status> {
    // TODO(session-lsp-tools): resolve the root, `client_for(root)`, `LspClient::symbols` for the
    // file or `LspClient::workspace_symbols` for a query, map each location as navigation does.
    Err(Status::unimplemented(
        "Symbols is not served yet — TODO(session-lsp-tools)",
    ))
}

/// What the language server reports wrong with the request's file.
pub(crate) async fn serve_diagnostics(
    _index: &WorkspaceIndex,
    _request: DiagnosticsRequest,
) -> Result<DiagnosticsResponse, Status> {
    // TODO(session-lsp-tools): resolve the root, `client_for(root)`, open the file,
    // `LspClient::diagnostics`, map each range to one-based byte coordinates.
    Err(Status::unimplemented(
        "Diagnostics is not served yet — TODO(session-lsp-tools)",
    ))
}
