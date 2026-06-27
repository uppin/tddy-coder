//! Build AuthService from daemon config.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tddy_github::{AuthServiceImpl, GitHubUser, RealGitHubProvider, StubGitHubProvider};
use tddy_rpc::ServiceEntry;
use tddy_service::AuthServiceServer;

use crate::config::DaemonConfig;
use crate::connection_service::SessionUserResolver;

/// Result of building auth: RPC entries and a resolver for session token -> GitHub login.
pub struct AuthBuildResult {
    pub entries: Vec<ServiceEntry>,
    pub user_resolver: Option<SessionUserResolver>,
}

/// Resolve the path where auth sessions are persisted across daemon restarts.
///
/// Honors the config-only tddy-home principle: the file lives under the daemon's resolved
/// `tddy_data_dir` (config → profile default → `$HOME/.tddy`), not a statically-derived
/// `$HOME/.tddy` that would ignore `DaemonConfig::tddy_data_dir`.
fn auth_session_persist_path(tddy_data_dir: &std::path::Path) -> PathBuf {
    tddy_data_dir.join("auth-sessions.json")
}

/// Build RPC entries for AuthService when GitHub is configured.
/// Returns entries and a user resolver for ConnectionService.
///
/// `tddy_data_dir` is the daemon's resolved tddy home (see `main.rs`), the single source of
/// truth for where persistent session state — including auth sessions — is stored.
pub fn build_auth_entries(
    config: &DaemonConfig,
    web_host: &str,
    web_port: u16,
    tddy_data_dir: &std::path::Path,
) -> AuthBuildResult {
    let github = match &config.github {
        Some(g) => g,
        None => {
            return AuthBuildResult {
                entries: vec![],
                user_resolver: None,
            };
        }
    };

    let sessions = Arc::new(Mutex::new(HashMap::<String, GitHubUser>::new()));
    let sessions_for_resolver = Arc::clone(&sessions);

    // Persist session tokens under the resolved tddy home so they survive daemon restarts.
    let persist_path = auth_session_persist_path(tddy_data_dir);

    let auth_entry = if github.stub.unwrap_or(false) {
        let client_id = github.client_id.as_deref().unwrap_or("stub-client-id");
        let callback_url = github
            .redirect_uri
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/auth/callback", web_host, web_port));
        let stub = StubGitHubProvider::new_with_callback(&callback_url, client_id);
        if let Some(ref codes) = github.stub_codes {
            for mapping in codes.split(',') {
                let parts: Vec<&str> = mapping.splitn(2, ':').collect();
                if parts.len() == 2 {
                    stub.register_code(
                        parts[0],
                        tddy_github::GitHubUser {
                            id: 1,
                            login: parts[1].to_string(),
                            avatar_url: format!("https://github.com/{}.png", parts[1]),
                            name: parts[1].to_string(),
                        },
                    );
                }
            }
        }
        let auth_service_impl =
            AuthServiceImpl::new_with_sessions_persisted(stub, sessions, persist_path.clone());
        let auth_server = AuthServiceServer::new(auth_service_impl);
        ServiceEntry {
            name: "auth.AuthService",
            service: Arc::new(auth_server) as Arc<dyn tddy_rpc::RpcService>,
        }
    } else if let (Some(id), Some(secret)) = (&github.client_id, &github.client_secret) {
        let redirect_uri = github
            .redirect_uri
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/auth/callback", web_host, web_port));
        let real = RealGitHubProvider::new(id, secret, &redirect_uri);
        let auth_service_impl =
            AuthServiceImpl::new_with_sessions_persisted(real, sessions, persist_path);
        let auth_server = AuthServiceServer::new(auth_service_impl);
        ServiceEntry {
            name: "auth.AuthService",
            service: Arc::new(auth_server) as Arc<dyn tddy_rpc::RpcService>,
        }
    } else {
        return AuthBuildResult {
            entries: vec![],
            user_resolver: None,
        };
    };

    let user_resolver: SessionUserResolver = Arc::new(move |token| {
        sessions_for_resolver
            .lock()
            .unwrap()
            .get(token)
            .map(|u| u.login.clone())
    });

    AuthBuildResult {
        entries: vec![auth_entry],
        user_resolver: Some(user_resolver),
    }
}
