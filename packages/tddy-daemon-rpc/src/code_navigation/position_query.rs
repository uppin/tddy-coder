//! The three position questions both the unary methods and the code session put to the index —
//! built in one place, so the two never ask them differently.

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tonic::transport::Channel;

/// A question about the symbol at `position` in `file`, in the index's terms, rooted at
/// `workspace_root`.
pub(crate) struct PositionQuery {
    pub(crate) workspace_root: String,
    pub(crate) file: String,
    pub(crate) position: Option<index::SourcePosition>,
}

impl PositionQuery {
    /// Where the symbol is defined, as the index answers it.
    pub(crate) async fn definition(
        self,
        client: &mut CodeIndexServiceClient<Channel>,
    ) -> Result<index::DefinitionResponse, tonic::Status> {
        let request = index::DefinitionRequest {
            workspace_root: self.workspace_root,
            file: self.file,
            position: self.position,
        };
        Ok(client.definition(request).await?.into_inner())
    }

    /// Every reference to the symbol, as the index answers it.
    pub(crate) async fn references(
        self,
        client: &mut CodeIndexServiceClient<Channel>,
    ) -> Result<index::ReferencesResponse, tonic::Status> {
        let request = index::ReferencesRequest {
            workspace_root: self.workspace_root,
            file: self.file,
            position: self.position,
        };
        Ok(client.references(request).await?.into_inner())
    }

    /// The symbol's hover text, as the index answers it.
    pub(crate) async fn hover(
        self,
        client: &mut CodeIndexServiceClient<Channel>,
    ) -> Result<index::HoverResponse, tonic::Status> {
        let request = index::HoverRequest {
            workspace_root: self.workspace_root,
            file: self.file,
            position: self.position,
        };
        Ok(client.hover(request).await?.into_inner())
    }
}
