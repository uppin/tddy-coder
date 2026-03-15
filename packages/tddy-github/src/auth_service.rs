use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use tddy_rpc::{Request, Response, Status};

use crate::provider::{GitHubOAuthProvider, GitHubUser};

use tddy_service::proto::auth::{
    AuthService as AuthServiceTrait, ExchangeCodeRequest, ExchangeCodeResponse,
    GetAuthStatusRequest, GetAuthStatusResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    GitHubUser as ProtoGitHubUser, LogoutRequest, LogoutResponse,
};

fn to_proto_user(user: &GitHubUser) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: user.id,
        login: user.login.clone(),
        avatar_url: user.avatar_url.clone(),
        name: user.name.clone(),
    }
}

/// Auth service implementation. Delegates OAuth to a GitHubOAuthProvider
/// and manages sessions in memory.
pub struct AuthServiceImpl<P: GitHubOAuthProvider> {
    provider: Arc<P>,
    sessions: Arc<Mutex<HashMap<String, GitHubUser>>>,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl<P: GitHubOAuthProvider> AuthServiceTrait for AuthServiceImpl<P> {
    async fn get_auth_url(
        &self,
        _request: Request<GetAuthUrlRequest>,
    ) -> Result<Response<GetAuthUrlResponse>, Status> {
        let (authorize_url, state) = self.provider.authorize_url();
        Ok(Response::new(GetAuthUrlResponse {
            authorize_url,
            state,
        }))
    }

    async fn exchange_code(
        &self,
        request: Request<ExchangeCodeRequest>,
    ) -> Result<Response<ExchangeCodeResponse>, Status> {
        let req = request.into_inner();
        let (_access_token, user) = self
            .provider
            .exchange_code(&req.code, &req.state)
            .await
            .map_err(Status::internal)?;

        let session_token = Uuid::new_v4().to_string();
        let proto_user = to_proto_user(&user);

        self.sessions
            .lock()
            .unwrap()
            .insert(session_token.clone(), user);

        Ok(Response::new(ExchangeCodeResponse {
            session_token,
            user: Some(proto_user),
        }))
    }

    async fn get_auth_status(
        &self,
        request: Request<GetAuthStatusRequest>,
    ) -> Result<Response<GetAuthStatusResponse>, Status> {
        let req = request.into_inner();
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(&req.session_token) {
            Some(user) => Ok(Response::new(GetAuthStatusResponse {
                authenticated: true,
                user: Some(to_proto_user(user)),
            })),
            None => Ok(Response::new(GetAuthStatusResponse {
                authenticated: false,
                user: None,
            })),
        }
    }

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        let req = request.into_inner();
        self.sessions.lock().unwrap().remove(&req.session_token);
        Ok(Response::new(LogoutResponse {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubGitHubProvider;
    use tddy_rpc::RpcBridge;
    use tddy_service::proto::auth::AuthServiceServer;

    fn setup() -> (StubGitHubProvider, GitHubUser) {
        let stub = StubGitHubProvider::new("https://github.com", "test-client-id");
        let user = GitHubUser {
            id: 123,
            login: "testuser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
            name: "Test User".to_string(),
        };
        (stub, user)
    }

    #[tokio::test]
    async fn full_auth_flow_via_rpc() {
        let (stub, user) = setup();
        stub.register_code("test-code", user);
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        // 1. GetAuthUrl
        let get_url_req = GetAuthUrlRequest {};
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&get_url_req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthUrl", &[msg])
            .await
            .expect("GetAuthUrl should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let auth_url_resp = <GetAuthUrlResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(auth_url_resp
            .authorize_url
            .contains("client_id=test-client-id"));
        let state = auth_url_resp.state;

        // 2. ExchangeCode
        let exchange_req = ExchangeCodeRequest {
            code: "test-code".to_string(),
            state: state.clone(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&exchange_req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await
            .expect("ExchangeCode should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let exchange_resp =
            <ExchangeCodeResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!exchange_resp.session_token.is_empty());
        assert_eq!(exchange_resp.user.as_ref().unwrap().login, "testuser");
        let session_token = exchange_resp.session_token;

        // 3. GetAuthStatus (authenticated)
        let status_req = GetAuthStatusRequest {
            session_token: session_token.clone(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&status_req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("GetAuthStatus should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(status_resp.authenticated);
        assert_eq!(status_resp.user.as_ref().unwrap().login, "testuser");

        // 4. Logout
        let logout_req = LogoutRequest {
            session_token: session_token.clone(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&logout_req),
            metadata: Default::default(),
        };
        bridge
            .handle_messages("auth.AuthService", "Logout", &[msg])
            .await
            .expect("Logout should succeed");

        // 5. GetAuthStatus (no longer authenticated)
        let status_req = GetAuthStatusRequest { session_token };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&status_req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("GetAuthStatus after logout should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!status_resp.authenticated);
        assert!(status_resp.user.is_none());
    }

    #[tokio::test]
    async fn get_auth_status_with_invalid_session() {
        let (stub, _) = setup();
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        let req = GetAuthStatusRequest {
            session_token: "nonexistent-token".to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!status_resp.authenticated);
    }
}
