//! Go-to-definition, find-references and hover, answered by a root's warm language server.
//!
//! Each answer is one `tddy_lsp::LspClient` query (`definition`, `references`, `hover`) on the
//! client [`WorkspaceIndex::client_for`] hands out for the request's root. What this module owns is
//! the translation either side of that query: the service's one-based byte `SourcePosition` into the
//! server's zero-based position, and each answered location back into a path relative to the root —
//! or an absolute path marked `outside_root` when the server points outside it.

use tddy_rpc::Status;

use crate::index::WorkspaceIndex;
use crate::proto::code_index::{
    DefinitionRequest, DefinitionResponse, HoverRequest, HoverResponse, ReferencesRequest,
    ReferencesResponse,
};

/// Where the symbol at the request's position is defined.
pub(crate) async fn serve_definition(
    _index: &WorkspaceIndex,
    _request: DefinitionRequest,
) -> Result<DefinitionResponse, Status> {
    // TODO(code-navigation): resolve the root, `client_for(root)`, `LspClient::definition`, map
    // the locations back to root-relative one-based byte ranges.
    Err(Status::unimplemented(
        "Definition is not served yet — TODO(code-navigation)",
    ))
}

/// Every reference to the symbol at the request's position.
pub(crate) async fn serve_references(
    _index: &WorkspaceIndex,
    _request: ReferencesRequest,
) -> Result<ReferencesResponse, Status> {
    // TODO(code-navigation): as `serve_definition`, through `LspClient::references`.
    Err(Status::unimplemented(
        "References is not served yet — TODO(code-navigation)",
    ))
}

/// The hover text of the symbol at the request's position.
pub(crate) async fn serve_hover(
    _index: &WorkspaceIndex,
    _request: HoverRequest,
) -> Result<HoverResponse, Status> {
    // TODO(code-navigation): resolve the root, `client_for(root)`, `LspClient::hover`.
    Err(Status::unimplemented(
        "Hover is not served yet — TODO(code-navigation)",
    ))
}
