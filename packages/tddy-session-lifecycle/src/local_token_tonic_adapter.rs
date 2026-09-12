//! Tonic adapter for `local_token.LocalTokenService` on the Unix-domain socket.
//!
//! Minting is handled here rather than delegated to the RpcService implementation because the peer
//! credential is only present on the UDS transport.

use std::sync::Arc;

use tddy_github::{GitHubUser, SessionTokenSigner};
use tonic::transport::server::UdsConnectInfo;

use crate::config::DaemonConfig;
use tddy_service::proto::local_token::{MintLocalTokenRequest, MintLocalTokenResponse};
use tddy_service::proto::tonic_local_token::local_token_service_server::LocalTokenService as TonicLocalTokenService;

/// Maps a peer uid (from SO_PEERCRED) to its OS username.
pub type UidToUsername = Arc<dyn Fn(u32) -> Option<String> + Send + Sync>;

/// Serves `MintLocalToken` from `SO_PEERCRED` on the daemon's local socket.
pub struct LocalTokenUdsTonicAdapter {
    config: Arc<DaemonConfig>,
    signer: Option<SessionTokenSigner>,
    uid_to_username: UidToUsername,
}

impl LocalTokenUdsTonicAdapter {
    pub fn new(
        config: Arc<DaemonConfig>,
        signer: Option<SessionTokenSigner>,
        uid_to_username: UidToUsername,
    ) -> Self {
        Self {
            config,
            signer,
            uid_to_username,
        }
    }
}

#[tonic::async_trait]
impl TonicLocalTokenService for LocalTokenUdsTonicAdapter {
    async fn mint_local_token(
        &self,
        request: tonic::Request<MintLocalTokenRequest>,
    ) -> Result<tonic::Response<MintLocalTokenResponse>, tonic::Status> {
        let uid = request
            .extensions()
            .get::<UdsConnectInfo>()
            .and_then(|info| info.peer_cred.as_ref())
            .map(|cred| cred.uid())
            .ok_or_else(|| {
                tonic::Status::permission_denied(
                    "local token minting requires a Unix-domain-socket peer credential",
                )
            })?;

        let login = self
            .config
            .local_token_login_for_uid(uid, self.uid_to_username.as_ref())
            .ok_or_else(|| {
                tonic::Status::permission_denied(format!(
                    "peer uid {uid} is not mapped to a configured user"
                ))
            })?;

        let signer = self.signer.as_ref().ok_or_else(|| {
            tonic::Status::failed_precondition("local token minting requires a configured signer")
        })?;

        let user = GitHubUser {
            id: 0,
            login: login.clone(),
            avatar_url: String::new(),
            name: login,
        };
        let session_token = signer.mint_access(&user);
        Ok(tonic::Response::new(MintLocalTokenResponse {
            session_token,
        }))
    }
}
