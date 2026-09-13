//! `local_token.LocalTokenService` — family Q. Minting only; peer identity is resolved by the UDS transport.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::{Request, Response, ServiceEntry, Status};
use tddy_service::proto::local_token::{
    LocalTokenService as LocalTokenServiceTrait, MintLocalTokenRequest, MintLocalTokenResponse,
};
use tddy_service::LocalTokenServiceServer;

static SIGNER: OnceLock<Arc<SessionTokenSigner>> = OnceLock::new();

/// Why local-token minting could not complete.
#[derive(Debug, thiserror::Error)]
pub enum LocalTokenError {
    #[error("local token minting requires a configured signer")]
    NoSigner,
}

/// Remember the signer the daemon's wiring layer configured. Called from [`build_local_token_entry`].
pub fn set_local_token_signer(signer: Arc<SessionTokenSigner>) {
    let _ = SIGNER.set(signer);
}

fn signer() -> Result<Arc<SessionTokenSigner>, LocalTokenError> {
    SIGNER.get().cloned().ok_or(LocalTokenError::NoSigner)
}

/// Mint a session token for an identity the transport already resolved.
pub fn mint_local_token(resolved_user: &str) -> Result<String, LocalTokenError> {
    let signer = signer()?;
    let login = resolved_user.trim();
    if login.is_empty() {
        return Err(LocalTokenError::NoSigner);
    }
    let user = GitHubUser {
        id: 0,
        login: login.to_string(),
        avatar_url: String::new(),
        name: login.to_string(),
    };
    Ok(signer.mint_access(&user))
}

struct LocalTokenServiceImpl;

#[async_trait]
impl LocalTokenServiceTrait for LocalTokenServiceImpl {
    async fn mint_local_token(
        &self,
        _request: Request<MintLocalTokenRequest>,
    ) -> Result<Response<MintLocalTokenResponse>, Status> {
        // The UDS tonic adapter resolves uid → login and calls [`mint_local_token`] directly today;
        // this RPC path exists for Connect-HTTP registration symmetry and future callers that pass
        // identity through request extensions instead.
        Err(Status::unimplemented(
            "MintLocalToken over RpcService requires transport-resolved identity; use the UDS adapter",
        ))
    }
}

/// The `local_token.LocalTokenService` entry the daemon's wiring layer registers.
pub fn build_local_token_entry(signer: Arc<SessionTokenSigner>) -> ServiceEntry {
    set_local_token_signer(signer);
    ServiceEntry {
        name: "local_token.LocalTokenService",
        service: Arc::new(LocalTokenServiceServer::new(LocalTokenServiceImpl))
            as Arc<dyn tddy_rpc::RpcService>,
    }
}
