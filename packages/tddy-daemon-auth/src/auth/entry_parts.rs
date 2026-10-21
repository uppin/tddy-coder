use crate::auth::auth_service_entry;
use crate::auth::github_provider_kind;
use crate::auth::probe_writable;
use crate::auth::GitHubProviderKind;
use crate::auth::LiveKitTokenServiceImpl;
use crate::signing_key::{auth_storage_looser_than_owner_only, SessionTokens};
use std::sync::Arc;
use tddy_credentials::SessionVaults;
use tddy_daemon_extensions::RoomTokenMinter;
use tddy_daemon_kernel::config::{DaemonConfig, GitHubConfig};
use tddy_daemon_kernel::SessionUserResolver;
use tddy_github::{LoginAdmission, RealGitHubProvider, StubGitHubProvider, TokenKind};
use tddy_rpc::ServiceEntry;
use tddy_service::LiveKitTokenServiceServer;

// TODO(restructure-engine): extracted by hand — `extract_method` refuses a range holding the early
// `return` the `None` arm made.
pub(super) fn provider_auth_entry(
    github: &GitHubConfig,
    redirect_uri: &str,
    tokens: &SessionTokens,
    credential_vaults: Option<Arc<SessionVaults>>,
    admission: Option<Arc<dyn LoginAdmission>>,
) -> Option<ServiceEntry> {
    let entry = match github_provider_kind(github)? {
        GitHubProviderKind::Stub => {
            let client_id = github.client_id.as_deref().unwrap_or("stub-client-id");
            let stub = StubGitHubProvider::new_with_callback(redirect_uri, client_id);
            if let Some(ref codes) = github.stub_codes {
                stub.register_code_mappings(codes);
            }
            auth_service_entry(stub, tokens, credential_vaults, admission)
        }
        // Two configurations, neither a fallback for the other. With a secret this is a
        // confidential client and serves the redirect flow exactly as it always has. Without one
        // it is a public client — the shape a desktop install ships in, since a secret inside a
        // downloadable application is public the day it ships — and signs in by the device flow,
        // which authenticates with the client id alone.
        GitHubProviderKind::Confidential { client_id, secret } => auth_service_entry(
            RealGitHubProvider::new(client_id, secret, redirect_uri),
            tokens,
            credential_vaults,
            admission,
        ),
        GitHubProviderKind::Public { client_id } => auth_service_entry(
            RealGitHubProvider::new_public(client_id),
            tokens,
            credential_vaults,
            admission,
        ),
    };
    Some(entry)
}

pub(super) fn probed_credential_vaults(
    config: &DaemonConfig,
    lifetimes: crate::vault_lifetimes::VaultLifetimes,
) -> anyhow::Result<Option<Arc<SessionVaults>>> {
    let credential_vaults = match config.auth_storage.as_ref() {
        Some(dir) => {
            probe_writable(dir).map_err(|e| {
                anyhow::anyhow!(
                    "config.auth_storage ({}) cannot hold credential vaults: {e}. \
                     Every GitHub login fails until it is writable by the daemon user.",
                    dir.display()
                )
            })?;
            // Said once, here, at startup — and not repaired: see
            // `auth_storage_looser_than_owner_only`.
            if let Some(warning) = auth_storage_looser_than_owner_only(dir) {
                log::warn!(target: crate::AUTH_LOG_TARGET, "{warning}");
            }
            Some(Arc::new(crate::vault_lifetimes::credential_vaults_in(
                dir, &lifetimes,
            )))
        }
        None => None,
    };
    Ok(credential_vaults)
}

pub(super) fn session_user_resolver(tokens: &SessionTokens) -> SessionUserResolver {
    let verifier = Arc::clone(tokens.verifier());
    let user_resolver: SessionUserResolver = Arc::new(move |token: &str| {
        verifier
            .verify_now(token)
            .ok()
            .filter(|c| c.kind == TokenKind::Access)
            .map(|c| c.login)
    });
    user_resolver
}

pub(super) fn livekit_token_service_entry(
    config: &DaemonConfig,
    minter: Arc<dyn RoomTokenMinter + 'static>,
    user_resolver: &SessionUserResolver,
) -> ServiceEntry {
    ServiceEntry {
        name: "auth.LiveKitTokenService",
        service: Arc::new(LiveKitTokenServiceServer::new(
            LiveKitTokenServiceImpl::new(user_resolver.clone(), Arc::new(config.clone()), minter),
        )) as Arc<dyn tddy_rpc::RpcService>,
    }
}
