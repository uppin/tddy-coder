use std::collections::HashMap;
use std::path::PathBuf;
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
/// and manages sessions in memory, optionally persisting them to disk.
pub struct AuthServiceImpl<P: GitHubOAuthProvider> {
    provider: Arc<P>,
    sessions: Arc<Mutex<HashMap<String, GitHubUser>>>,
    /// When set, the session store is loaded from this file on construction and
    /// written back after every `exchange_code` / `logout`.
    persist_path: Option<PathBuf>,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            persist_path: None,
        }
    }

    /// Create with a shared session store. Use when ConnectionService needs to resolve session tokens.
    pub fn new_with_sessions(
        provider: P,
        sessions: Arc<Mutex<HashMap<String, GitHubUser>>>,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            sessions,
            persist_path: None,
        }
    }

    /// Create with a shared session store and a disk-persistence path.
    ///
    /// If `path` already exists its contents are loaded into `sessions` immediately so that
    /// tokens issued before the previous daemon restart are still valid. The file is written
    /// (atomically via a sibling `.tmp` file) after every `exchange_code` and `logout`.
    /// Disk errors are logged-and-continued — they never fail an RPC.
    pub fn new_with_sessions_persisted(
        provider: P,
        sessions: Arc<Mutex<HashMap<String, GitHubUser>>>,
        path: PathBuf,
    ) -> Self {
        // Load previously-persisted sessions so tokens survive a daemon restart.
        if path.exists() {
            match std::fs::read_to_string(&path)
                .map_err(|e| format!("{e}"))
                .and_then(|s| serde_json::from_str::<HashMap<String, GitHubUser>>(&s).map_err(|e| format!("{e}")))
            {
                Ok(loaded) => {
                    log::info!("auth: loaded {} persisted session(s) from {}", loaded.len(), path.display());
                    *sessions.lock().unwrap() = loaded;
                }
                Err(e) => {
                    log::warn!("auth: could not load persisted sessions from {}: {}", path.display(), e);
                }
            }
        }
        Self {
            provider: Arc::new(provider),
            sessions,
            persist_path: Some(path),
        }
    }

    /// Persist the current session store to disk. Errors are logged, never propagated.
    fn persist(&self) {
        let Some(ref path) = self.persist_path else { return };
        let snapshot: HashMap<String, GitHubUser> = self.sessions.lock().unwrap().clone();
        let json = match serde_json::to_string_pretty(&snapshot) {
            Ok(j) => j,
            Err(e) => { log::warn!("auth: failed to serialize sessions: {e}"); return; }
        };
        // Atomic write: write to a sibling `.tmp` file then rename.
        let tmp = path.with_extension("json.tmp");
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("auth: could not create parent dir {}: {e}", parent.display());
                return;
            }
        }
        if let Err(e) = std::fs::write(&tmp, &json) {
            log::warn!("auth: failed to write tmp sessions file {}: {e}", tmp.display());
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            log::warn!("auth: failed to rename {} -> {}: {e}", tmp.display(), path.display());
        }
    }

    /// Get the GitHub user login for a session token. Used by ConnectionService for user mapping.
    pub fn get_user_login(&self, session_token: &str) -> Option<String> {
        self.sessions
            .lock()
            .unwrap()
            .get(session_token)
            .map(|u| u.login.clone())
    }

    /// Get a shared reference to the sessions store for use by ConnectionService.
    pub fn sessions(&self) -> Arc<Mutex<HashMap<String, GitHubUser>>> {
        Arc::clone(&self.sessions)
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
        self.persist();

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
        self.persist();
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
        // Given an auth service wired to a stub provider with a pre-registered code
        let (stub, user) = setup();
        stub.register_code("test-code", user);
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        // When executing the full OAuth flow: GetAuthUrl → ExchangeCode → GetAuthStatus → Logout → GetAuthStatus

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
        // Then after logout the session is no longer authenticated
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!status_resp.authenticated);
        assert!(status_resp.user.is_none());
    }

    #[tokio::test]
    async fn get_auth_status_with_invalid_session() {
        // Given an auth service with no active sessions
        let (stub, _) = setup();
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        // When checking status for a non-existent session token
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

        // Then the response indicates not authenticated
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!status_resp.authenticated);
    }

    // -------------------------------------------------------------------------
    // Persistence — session tokens survive a daemon restart (disk round-trip)
    // -------------------------------------------------------------------------

    /// Exchange a code and return the session token (shared test step).
    async fn do_exchange(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        code: &str,
        state: &str,
    ) -> String {
        let req = ExchangeCodeRequest { code: code.to_string(), state: state.to_string() };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await
            .expect("ExchangeCode should succeed");
        let chunks = match resp { tddy_rpc::ResponseBody::Complete(c) => c, _ => panic!("expected Complete") };
        <ExchangeCodeResponse as prost::Message>::decode(&chunks[0][..])
            .unwrap()
            .session_token
    }

    /// Check auth status and return (authenticated, login).
    async fn do_get_status(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        token: &str,
    ) -> (bool, Option<String>) {
        let req = GetAuthStatusRequest { session_token: token.to_string() };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("GetAuthStatus should succeed");
        let chunks = match resp { tddy_rpc::ResponseBody::Complete(c) => c, _ => panic!("expected Complete") };
        let r = <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        (r.authenticated, r.user.map(|u| u.login))
    }

    fn persisted_bridge(
        code: &str,
        path: std::path::PathBuf,
    ) -> RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>> {
        let (stub, user) = setup();
        stub.register_code(code, user);
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let service = AuthServiceImpl::new_with_sessions_persisted(stub, sessions, path);
        RpcBridge::new(AuthServiceServer::new(service))
    }

    #[tokio::test]
    async fn session_survives_daemon_restart() {
        // Given — first "daemon" run: exchange a code, which persists to disk
        let dir = tempfile::tempdir().expect("tempdir");
        let persist_path = dir.path().join("auth-sessions.json");

        let bridge1 = persisted_bridge("persist-code", persist_path.clone());

        // Obtain a valid state token via GetAuthUrl
        let url_req = GetAuthUrlRequest {};
        let msg = tddy_rpc::RpcMessage { payload: prost::Message::encode_to_vec(&url_req), metadata: Default::default() };
        let resp = bridge1.handle_messages("auth.AuthService", "GetAuthUrl", &[msg]).await.unwrap();
        let chunks = match resp { tddy_rpc::ResponseBody::Complete(c) => c, _ => panic!() };
        let auth_url_resp = <GetAuthUrlResponse as prost::Message>::decode(&chunks[0][..]).unwrap();

        let token = do_exchange(&bridge1, "persist-code", &auth_url_resp.state).await;
        assert!(!token.is_empty(), "should have received a session token");

        let (auth, _login) = do_get_status(&bridge1, &token).await;
        assert!(auth, "should be authenticated on first service");

        // When — "daemon restarts": drop the first service, build a new one from the same persist file
        drop(bridge1);

        // New service: no code registered, just loads the file
        let (stub2, _) = setup();
        let sessions2 = Arc::new(Mutex::new(HashMap::new()));
        let service2 = AuthServiceImpl::new_with_sessions_persisted(stub2, sessions2, persist_path);
        let bridge2 = RpcBridge::new(AuthServiceServer::new(service2));

        // Then — the old token is still valid after the "restart"
        let (auth2, login2) = do_get_status(&bridge2, &token).await;
        assert!(auth2, "session token should survive a daemon restart");
        assert_eq!(login2.as_deref(), Some("testuser"));
    }

    #[tokio::test]
    async fn logout_removes_session_from_disk() {
        // Given — a persisted session
        let dir = tempfile::tempdir().expect("tempdir");
        let persist_path = dir.path().join("auth-sessions.json");

        let bridge = persisted_bridge("logout-code", persist_path.clone());

        let url_req = GetAuthUrlRequest {};
        let msg = tddy_rpc::RpcMessage { payload: prost::Message::encode_to_vec(&url_req), metadata: Default::default() };
        let resp = bridge.handle_messages("auth.AuthService", "GetAuthUrl", &[msg]).await.unwrap();
        let chunks = match resp { tddy_rpc::ResponseBody::Complete(c) => c, _ => panic!() };
        let auth_url_resp = <GetAuthUrlResponse as prost::Message>::decode(&chunks[0][..]).unwrap();

        let token = do_exchange(&bridge, "logout-code", &auth_url_resp.state).await;

        // When — logout
        let logout_req = LogoutRequest { session_token: token.clone() };
        let msg = tddy_rpc::RpcMessage { payload: prost::Message::encode_to_vec(&logout_req), metadata: Default::default() };
        bridge.handle_messages("auth.AuthService", "Logout", &[msg]).await.expect("logout should succeed");

        // Then — a new "daemon" (fresh service loading the same file) no longer recognises the token
        drop(bridge);

        let (stub2, _) = setup();
        let sessions2 = Arc::new(Mutex::new(HashMap::new()));
        let service2 = AuthServiceImpl::new_with_sessions_persisted(stub2, sessions2, persist_path);
        let bridge2 = RpcBridge::new(AuthServiceServer::new(service2));

        let (auth_after, _) = do_get_status(&bridge2, &token).await;
        assert!(!auth_after, "logged-out session should not be valid after restart");
    }
}
