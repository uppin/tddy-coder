//! Token service implementation for LiveKit token generation via RPC.

use async_trait::async_trait;
use std::sync::Arc;

use tddy_rpc::{Request, Response, Status};

use crate::proto::token::{
    GenerateTokenRequest, GenerateTokenResponse, RefreshTokenRequest, RefreshTokenResponse,
    TokenService as TokenServiceTrait,
};

/// Trait for providing LiveKit tokens. Implementations delegate to credential holders
/// (e.g. TokenGenerator) without exposing credentials to the service layer.
pub trait TokenProvider: Send + Sync + 'static {
    /// Generate a JWT token for the given room and identity.
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String>;
    /// Token TTL in seconds.
    fn ttl_seconds(&self) -> u64;
}

/// Token service implementation. Delegates to a TokenProvider.
pub struct TokenServiceImpl<P: TokenProvider> {
    provider: Arc<P>,
}

impl<P: TokenProvider> TokenServiceImpl<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }
}

#[async_trait]
impl<P: TokenProvider> TokenServiceTrait for TokenServiceImpl<P> {
    async fn generate_token(
        &self,
        request: Request<GenerateTokenRequest>,
    ) -> Result<Response<GenerateTokenResponse>, Status> {
        let req = request.into_inner();
        let token = self
            .provider
            .generate_token(&req.room, &req.identity)
            .map_err(Status::internal)?;
        Ok(Response::new(GenerateTokenResponse {
            token,
            ttl_seconds: self.provider.ttl_seconds(),
        }))
    }

    async fn refresh_token(
        &self,
        request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        let req = request.into_inner();
        let token = self
            .provider
            .generate_token(&req.room, &req.identity)
            .map_err(Status::internal)?;
        Ok(Response::new(RefreshTokenResponse {
            token,
            ttl_seconds: self.provider.ttl_seconds(),
        }))
    }
}
