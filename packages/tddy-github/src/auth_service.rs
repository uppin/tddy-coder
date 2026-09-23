use std::sync::Arc;

use async_trait::async_trait;

use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SecretString, SessionVaults, UnlockKey, VaultError,
    VaultState, MIN_PASSPHRASE_CHARS,
};
use tddy_rpc::{Request, RequestTransport, Response, Status};

use crate::provider::{DeviceLoginPoll, GitHubOAuthProvider, GitHubUser};
use crate::session_token_v2::{
    SessionClaims, SessionTokenAuthority, SessionTokenSigner, TokenKind,
};

use tddy_service::proto::auth::{
    AuthService as AuthServiceTrait, DeviceLoginState, ExchangeCodeRequest, ExchangeCodeResponse,
    GetAuthStatusRequest, GetAuthStatusResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    GitHubUser as ProtoGitHubUser, LogoutRequest, LogoutResponse, PollDeviceLoginRequest,
    PollDeviceLoginResponse, RefreshSessionRequest, RefreshSessionResponse, ResetVaultRequest,
    ResetVaultResponse, StartDeviceLoginRequest, StartDeviceLoginResponse, UnlockVaultRequest,
    UnlockVaultResponse, VaultState as ProtoVaultState,
};

fn to_proto_user(user: &GitHubUser) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: user.id,
        login: user.login.clone(),
        avatar_url: user.avatar_url.clone(),
        name: user.name.clone(),
    }
}

fn proto_user_from_claims(claims: &SessionClaims) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: claims.id,
        login: claims.login.clone(),
        avatar_url: claims.avatar_url.clone(),
        name: claims.name.clone(),
    }
}

/// Whether this daemon admits a login GitHub has just vouched for.
///
/// Asked once per completed sign-in, whichever flow completed it, after GitHub has confirmed who
/// the user is and before anything is retained or minted. `AuthServiceImpl` knows nothing about
/// the daemon's `users:` map; this is the narrow seam through which the daemon that does may act
/// on a login — enrol it, admit it, or refuse it — before a session exists for it. A refusal fails
/// the login with the returned status, and nothing is retained for it.
///
/// `transport` is how the completing call reached this daemon, as the host that received it
/// stamped it — never anything the caller wrote — so an admission may act differently for the
/// person at the machine than for a caller in a LiveKit room or on a socket.
pub trait LoginAdmission: Send + Sync {
    fn admit(&self, github_login: &str, transport: RequestTransport) -> Result<(), Status>;
}

/// Auth service implementation. Delegates OAuth to a GitHubOAuthProvider and issues stateless
/// session tokens signed with this daemon's own key (see [`crate::session_token_v2`]). No session
/// state is kept server-side: a token is verifiable by any daemon that can resolve the key it names.
pub struct AuthServiceImpl<P: GitHubOAuthProvider> {
    provider: Arc<P>,
    /// When set, sign-in mints tokens and status/refresh verify them. When `None`, authentication
    /// is non-functional: minting fails and every token is rejected.
    signing: Option<Signing>,
    /// When set, a real provider's GitHub access token is sealed into the operator's credential
    /// vault on login so the server can later act on their behalf (e.g. read their PRs), and each
    /// session lineage is handed an unlock key its refreshes reopen the vault with after a
    /// restart. Separate from `signing` on purpose: the GitHub token never enters the session
    /// token and is never returned to the client.
    credential_vaults: Option<Arc<SessionVaults>>,
    /// When set, asked whether each completed login is admitted before it is retained or minted.
    /// Unset admits every login GitHub vouches for, and leaves authorization to the RPCs that
    /// resolve the caller's OS user.
    admission: Option<Arc<dyn LoginAdmission>>,
}

/// The provider a GitHub login's access token is filed under in the credential vault.
pub const GITHUB_PROVIDER: &str = "github";

/// What a signed service mints with and verifies through.
struct Signing {
    /// This daemon's own key: every token the service hands out is signed with it.
    signer: SessionTokenSigner,
    /// Decides whether a presented token is genuine, whichever daemon signed it — so a status
    /// check or a refresh accepts a peer's token exactly as far as the fleet's key directory does.
    authority: Arc<dyn SessionTokenAuthority>,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// Create without a signer. Authentication is non-functional — minting fails and every token
    /// is rejected. A process that can hand out an authorize URL but holds no identity of its own
    /// (a session coder's web surface) serves this.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            signing: None,
            credential_vaults: None,
            admission: None,
        }
    }

    /// Create a service that mints with `signer` and verifies through `authority`.
    ///
    /// Tokens are self-describing — no shared or persisted session store — and `authority` is
    /// what lets a token minted by one daemon be recognised by another.
    pub fn new_signed(
        provider: P,
        signer: SessionTokenSigner,
        authority: Arc<dyn SessionTokenAuthority>,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            signing: Some(Signing { signer, authority }),
            credential_vaults: None,
            admission: None,
        }
    }

    /// Seal each real login's GitHub access token into that operator's vault in `vaults`
    /// (builder). Without vaults the token is dropped at the end of the exchange, and GitHub-backed
    /// reads report themselves unavailable.
    pub fn with_credential_vaults(mut self, vaults: Arc<SessionVaults>) -> Self {
        self.credential_vaults = Some(vaults);
        self
    }

    /// Ask `admission` about every completed login before it is retained or minted (builder).
    pub fn with_login_admission(mut self, admission: Arc<dyn LoginAdmission>) -> Self {
        self.admission = Some(admission);
        self
    }

    /// The vaults this service retains real logins' credentials in — `None` when it keeps none:
    /// no `auth_storage`, or a stub provider, whose token is synthetic and different on every
    /// exchange, so a demo holds no credential by construction (D12).
    fn retaining_vaults(&self) -> Option<&SessionVaults> {
        self.credential_vaults
            .as_deref()
            .filter(|_| self.provider.issues_usable_access_token())
    }

    /// Where `login`'s vault stands, as a response reports it.
    fn vault_state_of(&self, login: &str) -> ProtoVaultState {
        self.retaining_vaults()
            .map_or(ProtoVaultState::None, |vaults| {
                to_proto_state(vaults.state(login))
            })
    }

    /// Retain this login's GitHub token, and say where the operator's vault stands — with the
    /// signing-in lineage's unlock key in its wire form when the vault is open, `""` otherwise.
    ///
    /// Open: the token is sealed and the lineage is handed an unlock slot, with no prompt. Closed
    /// (`Locked` after a restart, `Uninitialized` before the first passphrase): the token is held
    /// in memory for the vault, and the state tells the client to prompt. Either way the login is
    /// **reported**, never silently half-done: a session minted while its token cannot be read
    /// says so. A write that fails is still a failed login — the operator would otherwise appear
    /// signed in while every GitHub-backed read reported itself unavailable.
    fn retain_the_login_credential(
        &self,
        user: &GitHubUser,
        access_token: &str,
    ) -> Result<(ProtoVaultState, String), Status> {
        let Some(vaults) = self.retaining_vaults() else {
            return Ok((ProtoVaultState::None, String::new()));
        };
        let login = &user.login;
        let retained = vaults
            .retain(login, github_record(user, access_token)?)
            .map_err(|e| refused_by_the_vault(login, "retain the GitHub access token", e))?;
        if retained.state != VaultState::Open {
            log::info!(
                target: "tddy_github::auth_service",
                "the credential vault of '{login}' is {:?} on this daemon; its GitHub token is held \
                 in memory until the vault is unlocked",
                retained.state
            );
        }
        Ok((
            to_proto_state(retained.state),
            retained
                .unlock_key
                .map(|unlock| unlock.to_wire())
                .unwrap_or_default(),
        ))
    }

    /// Reopen the refreshing user's vault through the unlock key their lineage presented, and
    /// return the rotated key with the vault's state — or `""` and whatever state the vault is in
    /// when none was presented or it no longer opens its slot.
    ///
    /// Never fails the refresh. The session token and the vault are separate things: refusing
    /// the refresh would sign the operator out of everything for a credential-store problem.
    /// A key that does not open its slot is logged, and the state says what opens the vault now.
    fn reopen_the_vault(&self, login: &str, presented: &str) -> (String, ProtoVaultState) {
        let Some(vaults) = self.retaining_vaults() else {
            return (String::new(), ProtoVaultState::None);
        };
        if presented.is_empty() {
            return (String::new(), to_proto_state(vaults.state(login)));
        }
        let reopened = UnlockKey::from_wire(presented)
            .filter(|unlock| unlock.subject() == login)
            .ok_or(VaultError::Locked)
            .and_then(|unlock| vaults.reopen(&unlock));
        match reopened {
            Ok(rotated) => (rotated.to_wire(), ProtoVaultState::Open),
            Err(e) => {
                log::warn!(
                    target: "tddy_github::auth_service",
                    "the vault unlock key presented at session refresh for login '{login}' did not \
                     reopen its credential vault ({e}); it stays as it is until its passphrase is \
                     given"
                );
                (String::new(), to_proto_state(vaults.state(login)))
            }
        }
    }

    /// The GitHub login an access token was minted for — `unauthenticated` for anything else.
    async fn caller_login(&self, session_token: &str) -> Result<String, Status> {
        let Some(ref signing) = self.signing else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        let claims = signing
            .authority
            .verify(session_token)
            .await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        if claims.kind != TokenKind::Access {
            return Err(Status::unauthenticated(
                "session token: not an access token",
            ));
        }
        Ok(claims.login)
    }

    /// The vaults an unlock or a reset acts on, or why there is nothing to act on.
    fn vaults_to_unlock(&self) -> Result<&SessionVaults, Status> {
        self.retaining_vaults().ok_or_else(|| {
            Status::failed_precondition("this daemon keeps no credential vault for this login")
        })
    }
}

/// A passphrase a vault may be created under — long enough that Argon2id's cost means something.
fn check_new_passphrase(passphrase: &SecretString) -> Result<(), Status> {
    if passphrase.expose().chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(Status::invalid_argument(format!(
            "a credential vault passphrase is at least {MIN_PASSPHRASE_CHARS} characters"
        )));
    }
    Ok(())
}

fn to_proto_state(state: VaultState) -> ProtoVaultState {
    match state {
        VaultState::Open => ProtoVaultState::Open,
        VaultState::Locked => ProtoVaultState::Locked,
        VaultState::Uninitialized => ProtoVaultState::Uninitialized,
    }
}

/// What a login's GitHub token is sealed as. The GitHub login is both the vault's subject and
/// the account, until a second GitHub account per user exists (`#keyring` 8/9).
///
/// A clock before the Unix epoch is refused rather than recorded as `0`: `updated_at` is what the
/// Accounts screen (`#keyring` 4/9) orders and ages records by, and a silent zero would present a
/// fresh credential as the oldest one held.
fn github_record(user: &GitHubUser, access_token: &str) -> Result<CredentialRecord, Status> {
    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            log::error!(
                target: "tddy_github::auth_service",
                "the daemon's clock reads before the Unix epoch ({e}); not retaining a GitHub token \
                 with no valid timestamp"
            );
            Status::internal("the daemon's clock is wrong; cannot retain the GitHub access token")
        })?
        .as_secs();
    Ok(CredentialRecord {
        provider: ProviderId::new(GITHUB_PROVIDER),
        account: AccountId::new(&user.login),
        label: if user.name.is_empty() {
            user.login.clone()
        } else {
            user.name.clone()
        },
        secret: SecretString::new(access_token),
        metadata: [
            (GITHUB_ID_METADATA.to_string(), user.id.to_string()),
            (AVATAR_URL_METADATA.to_string(), user.avatar_url.clone()),
        ]
        .into(),
        updated_at,
    })
}

/// The metadata key a GitHub record carries its numeric user id under.
pub const GITHUB_ID_METADATA: &str = "github_id";
/// The metadata key a GitHub record carries its avatar URL under.
pub const AVATAR_URL_METADATA: &str = "avatar_url";

/// The status a vault operation's refusal is reported with. `doing` names the operation, as the
/// client's message does: "could not {doing} for login '…'".
///
/// Only `Io` names server-side detail — the path it could not write, the OS error behind it — so
/// that one goes to the daemon log and the client is told only *what* failed and for which login.
/// Every other refusal is told as it is, because the operator's remedy depends on which it was:
/// `Locked` is a passphrase to retry, `FormatMismatch` is an upgrade.
fn refused_by_the_vault(login: &str, doing: &str, error: VaultError) -> Status {
    match error {
        VaultError::Io(detail) => {
            log::error!(
                target: "tddy_github::auth_service",
                "could not {doing} for login '{login}': {detail}"
            );
            Status::internal(format!("could not {doing} for login '{login}'"))
        }
        refusal => {
            log::warn!(
                target: "tddy_github::auth_service",
                "the credential vault refused to {doing} for '{login}': {refusal}"
            );
            Status::failed_precondition(refusal.to_string())
        }
    }
}

/// What a completed sign-in hands the client, whichever flow completed it.
struct MintedSession {
    session_token: String,
    user: ProtoGitHubUser,
    refresh_token: String,
    vault_unlock_key: String,
    vault_state: ProtoVaultState,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// Finish a sign-in GitHub has vouched for: retain its access token, then mint the session.
    ///
    /// One implementation for both flows, so a device login and a redirect login cannot drift
    /// into producing different sessions or retaining the credential under different rules.
    ///
    /// `transport` is the completing call's, as its host stamped it; admission is told it.
    fn complete_login(
        &self,
        access_token: &str,
        user: &GitHubUser,
        transport: RequestTransport,
    ) -> Result<MintedSession, Status> {
        // Admission first: a login this daemon refuses must leave nothing behind — no retained
        // GitHub credential, no session.
        if let Some(ref admission) = self.admission {
            admission.admit(&user.login, transport)?;
        }

        // Signed mode: return a stateless token any daemon that can resolve this daemon's key
        // verifies. No server-side session state is kept.
        let Some(Signing { ref signer, .. }) = self.signing else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };

        // Retain the operator's own credential for later server-side GitHub reads — or fail the
        // login, see `retain_the_login_credential`. After the signing check, so a service that
        // cannot mint a session leaves no unlock slot behind for a lineage that will never exist.
        let (vault_state, vault_unlock_key) =
            self.retain_the_login_credential(user, access_token)?;

        // A short-lived access token for RPCs plus a long-lived refresh token to mint further
        // access tokens without re-login.
        Ok(MintedSession {
            session_token: signer.mint_access(user),
            user: to_proto_user(user),
            refresh_token: signer.mint_refresh(user),
            vault_unlock_key,
            vault_state,
        })
    }
}

#[async_trait]
impl<P: GitHubOAuthProvider> AuthServiceTrait for AuthServiceImpl<P> {
    async fn get_auth_url(
        &self,
        _request: Request<GetAuthUrlRequest>,
    ) -> Result<Response<GetAuthUrlResponse>, Status> {
        // The only refusal is a provider that cannot complete the redirect flow at all, which is a
        // property of how this daemon is configured rather than of the request.
        let (authorize_url, state) = self
            .provider
            .authorize_url()
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(GetAuthUrlResponse {
            authorize_url,
            state,
        }))
    }

    async fn exchange_code(
        &self,
        request: Request<ExchangeCodeRequest>,
    ) -> Result<Response<ExchangeCodeResponse>, Status> {
        let transport = request.metadata().transport();
        let req = request.into_inner();
        let (access_token, user) = self
            .provider
            .exchange_code(&req.code, &req.state)
            .await
            .map_err(Status::internal)?;

        let session = self.complete_login(&access_token, &user, transport)?;
        Ok(Response::new(ExchangeCodeResponse {
            session_token: session.session_token,
            user: Some(session.user),
            refresh_token: session.refresh_token,
            vault_unlock_key: session.vault_unlock_key,
            vault_state: session.vault_state as i32,
        }))
    }

    async fn get_auth_status(
        &self,
        request: Request<GetAuthStatusRequest>,
    ) -> Result<Response<GetAuthStatusResponse>, Status> {
        let req = request.into_inner();
        // Only an access-kind token authenticates a session — a refresh token is a minting
        // credential, never proof of an authenticated session (matches the daemon RPC resolver).
        let claims = match &self.signing {
            Some(signing) => signing.authority.verify(&req.session_token).await.ok(),
            None => None,
        }
        .filter(|claims| claims.kind == TokenKind::Access);
        Ok(Response::new(match claims {
            Some(claims) => GetAuthStatusResponse {
                authenticated: true,
                user: Some(proto_user_from_claims(&claims)),
                vault_state: self.vault_state_of(&claims.login) as i32,
            },
            None => GetAuthStatusResponse::default(),
        }))
    }

    async fn refresh_session(
        &self,
        request: Request<RefreshSessionRequest>,
    ) -> Result<Response<RefreshSessionResponse>, Status> {
        let req = request.into_inner();
        let Some(Signing {
            ref signer,
            ref authority,
        }) = self.signing
        else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        // Only a currently-valid refresh token can extend a session; an expired/forged one forces
        // re-login.
        let claims = authority
            .verify(&req.refresh_token)
            .await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        // A short-lived access token must not be usable to mint — only a refresh token can.
        if claims.kind != TokenKind::Refresh {
            return Err(Status::unauthenticated(
                "session token: not a refresh token",
            ));
        }
        let user = claims.user();
        let (vault_unlock_key, vault_state) =
            self.reopen_the_vault(&user.login, &req.vault_unlock_key);
        // Mint a fresh access token plus a slid refresh token (fresh 7-day window), signed with
        // this daemon's own key whichever daemon signed the refresh token it was given.
        let session_token = signer.mint_access(&user);
        let refresh_token = signer.mint_refresh(&user);
        Ok(Response::new(RefreshSessionResponse {
            session_token,
            user: Some(to_proto_user(&user)),
            refresh_token,
            vault_unlock_key,
            vault_state: vault_state as i32,
        }))
    }

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        // Signed session tokens are stateless — logout is client-side (the client discards its
        // token). What the daemon does hold is this lineage's unlock slot in the vault, and that
        // is removed. The key itself proves the lineage, so an expired access token does not keep
        // the slot alive.
        let req = request.into_inner();
        if let (Some(vaults), Some(unlock)) = (
            self.credential_vaults.as_ref(),
            UnlockKey::from_wire(&req.vault_unlock_key),
        ) {
            if let Err(e) = vaults.forget(&unlock) {
                log::warn!(
                    target: "tddy_github::auth_service",
                    "logout of '{}' left its vault unlock slot in place: {e}",
                    unlock.subject()
                );
            }
        }
        Ok(Response::new(LogoutResponse {}))
    }

    async fn start_device_login(
        &self,
        _request: Request<StartDeviceLoginRequest>,
    ) -> Result<Response<StartDeviceLoginResponse>, Status> {
        // Nothing is remembered here — the device code is the client's to hold and present again.
        let started = self
            .provider
            .start_device_login()
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(StartDeviceLoginResponse {
            device_code: started.device_code,
            user_code: started.user_code,
            verification_uri: started.verification_uri,
            expires_in_seconds: started.expires_in_seconds,
            interval_seconds: started.interval_seconds,
        }))
    }

    async fn poll_device_login(
        &self,
        request: Request<PollDeviceLoginRequest>,
    ) -> Result<Response<PollDeviceLoginResponse>, Status> {
        let transport = request.metadata().transport();
        let req = request.into_inner();
        let polled = self
            .provider
            .poll_device_login(&req.device_code)
            .await
            .map_err(Status::internal)?;
        let without_session = |state: DeviceLoginState| PollDeviceLoginResponse {
            state: state as i32,
            ..Default::default()
        };
        Ok(Response::new(match polled {
            DeviceLoginPoll::Pending => without_session(DeviceLoginState::Pending),
            DeviceLoginPoll::SlowDown { interval_seconds } => PollDeviceLoginResponse {
                interval_seconds,
                ..without_session(DeviceLoginState::SlowDown)
            },
            DeviceLoginPoll::Denied => without_session(DeviceLoginState::Denied),
            DeviceLoginPoll::Expired => without_session(DeviceLoginState::Expired),
            DeviceLoginPoll::Complete { access_token, user } => {
                let session = self.complete_login(&access_token, &user, transport)?;
                PollDeviceLoginResponse {
                    state: DeviceLoginState::Complete as i32,
                    interval_seconds: 0,
                    session_token: session.session_token,
                    user: Some(session.user),
                    refresh_token: session.refresh_token,
                    vault_unlock_key: session.vault_unlock_key,
                    vault_state: session.vault_state as i32,
                }
            }
        }))
    }

    async fn unlock_vault(
        &self,
        request: Request<UnlockVaultRequest>,
    ) -> Result<Response<UnlockVaultResponse>, Status> {
        let req = request.into_inner();
        let passphrase = SecretString::new(req.passphrase);
        let login = self.caller_login(&req.session_token).await?;
        let vaults = self.vaults_to_unlock()?;
        let opened = if req.create {
            check_new_passphrase(&passphrase)?;
            vaults.create(&login, &passphrase)
        } else {
            vaults.unlock(&login, &passphrase)
        };
        let unlock =
            opened.map_err(|e| refused_by_the_vault(&login, "open the credential vault", e))?;
        Ok(Response::new(UnlockVaultResponse {
            vault_state: ProtoVaultState::Open as i32,
            vault_unlock_key: unlock.to_wire(),
        }))
    }

    async fn reset_vault(
        &self,
        request: Request<ResetVaultRequest>,
    ) -> Result<Response<ResetVaultResponse>, Status> {
        let req = request.into_inner();
        let new_passphrase = SecretString::new(req.new_passphrase);
        let login = self.caller_login(&req.session_token).await?;
        let vaults = self.vaults_to_unlock()?;
        check_new_passphrase(&new_passphrase)?;
        let reset = vaults
            .reset(&login, &new_passphrase)
            .map_err(|e| refused_by_the_vault(&login, "reset the credential vault", e))?;
        if let Some(ref aside) = reset.set_aside {
            log::warn!(
                target: "tddy_github::auth_service",
                "the credential vault of '{login}' was reset; the old one is kept at {}",
                aside.display()
            );
        }
        Ok(Response::new(ResetVaultResponse {
            vault_state: ProtoVaultState::Open as i32,
            vault_unlock_key: reset.unlock_key.to_wire(),
        }))
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
    async fn get_auth_status_with_invalid_session() {
        // Given an auth service with no signing key configured
        let (stub, _) = setup();
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        // When checking status for an unverifiable session token
        let req = GetAuthStatusRequest {
            session_token: "nonexistent-token".to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
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
    // Shared RPC step helpers
    // -------------------------------------------------------------------------

    /// Exchange a code and return the session token (shared test step).
    async fn do_exchange(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        code: &str,
        state: &str,
    ) -> String {
        let req = ExchangeCodeRequest {
            code: code.to_string(),
            state: state.to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await
            .expect("ExchangeCode should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        <ExchangeCodeResponse as prost::Message>::decode(&chunks[0][..])
            .unwrap()
            .session_token
    }

    /// Check auth status and return (authenticated, login).
    async fn do_get_status(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        token: &str,
    ) -> (bool, Option<String>) {
        let req = GetAuthStatusRequest {
            session_token: token.to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("GetAuthStatus should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let r = <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        (r.authenticated, r.user.map(|u| u.login))
    }

    // -------------------------------------------------------------------------
    // Cross-daemon session tokens — a token minted by one daemon is verifiable by another that has
    // learned the first one's public key, with no shared secret and no shared session store.
    // -------------------------------------------------------------------------

    /// A daemon's signing key. Fixed bytes, so a failure names a behaviour rather than a seed.
    fn a_daemon_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn a_signer_for(key: &SigningKey) -> SessionTokenSigner {
        SessionTokenSigner::new(key.clone())
    }

    /// A signed service minting with `key` and admitting the tokens of every key in `trusted`.
    fn a_signed_service(
        provider: StubGitHubProvider,
        key: &SigningKey,
        trusted: &[&SigningKey],
    ) -> AuthServiceImpl<StubGitHubProvider> {
        AuthServiceImpl::new_signed(
            provider,
            a_signer_for(key),
            Arc::new(TrustsKeys(
                trusted.iter().map(|key| key.verifying_key()).collect(),
            )),
        )
    }

    fn signed_bridge(
        code: &str,
        key: &SigningKey,
        trusted: &[&SigningKey],
    ) -> RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>> {
        let (stub, user) = setup();
        stub.register_code(code, user);
        RpcBridge::new(AuthServiceServer::new(a_signed_service(stub, key, trusted)))
    }

    /// A daemon's view of the fleet, as a fixed set of public keys it has learned.
    struct TrustsKeys(Vec<VerifyingKey>);

    #[async_trait]
    impl SessionTokenAuthority for TrustsKeys {
        async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
            let key_id = SessionTokenVerifier::key_id_of(token)?;
            let key = self
                .0
                .iter()
                .find(|key| KeyId::of(key) == key_id)
                .ok_or(SessionTokenError::UnknownKeyId(key_id))?;
            SessionTokenVerifier::verify(token, key, SystemTime::now())
        }
    }

    /// The claims `token` carries, checked under `key`.
    fn claims_under(key: &SigningKey, token: &str) -> SessionClaims {
        SessionTokenVerifier::verify(token, &key.verifying_key(), SystemTime::now())
            .expect("a token this daemon minted verifies under its key")
    }

    async fn do_get_auth_url_state(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
    ) -> String {
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&GetAuthUrlRequest {}),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthUrl", &[msg])
            .await
            .expect("GetAuthUrl should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        <GetAuthUrlResponse as prost::Message>::decode(&chunks[0][..])
            .unwrap()
            .state
    }

    #[tokio::test]
    async fn a_token_minted_by_one_daemon_is_authenticated_by_another_that_has_learned_its_key() {
        // Given one daemon that mints a session token through the GitHub login flow
        let (daemon_a, daemon_b) = (a_daemon_key(7), a_daemon_key(9));
        let serving = signed_bridge("login-code", &daemon_a, &[&daemon_a]);
        let state = do_get_auth_url_state(&serving).await;
        let token = do_exchange(&serving, "login-code", &state).await;

        // When a *different* daemon — its own key, its own service, no shared session store and no
        // shared secret — checks that token, having learned the first daemon's public key
        let (peer_stub, _) = setup();
        let peer = RpcBridge::new(AuthServiceServer::new(a_signed_service(
            peer_stub,
            &daemon_b,
            &[&daemon_b, &daemon_a],
        )));
        let status = do_get_status(&peer, &token).await;

        // Then the peer authenticates it from the signature and the signer's public key alone
        assert_eq!(
            status,
            (true, Some("testuser".to_string())),
            "a peer daemon must accept a token minted by a daemon whose key it has learned"
        );
    }

    #[tokio::test]
    async fn a_token_minted_by_one_daemon_is_refused_by_another_that_has_not_learned_its_key() {
        // Given one daemon that mints a session token through the GitHub login flow
        let (daemon_a, daemon_b) = (a_daemon_key(7), a_daemon_key(9));
        let serving = signed_bridge("login-code", &daemon_a, &[&daemon_a]);
        let state = do_get_auth_url_state(&serving).await;
        let token = do_exchange(&serving, "login-code", &state).await;

        // When a daemon that knows only its own key checks it
        let (peer_stub, _) = setup();
        let peer = RpcBridge::new(AuthServiceServer::new(a_signed_service(
            peer_stub,
            &daemon_b,
            &[&daemon_b],
        )));
        let status = do_get_status(&peer, &token).await;

        // Then it is not authenticated — there is no shared secret left to fall back on
        assert_eq!(status, (false, None));
    }

    // -------------------------------------------------------------------------
    // Signed-token minting, refresh, and the "no signer configured" guard.
    // -------------------------------------------------------------------------

    use crate::session_token_v2::{
        KeyId, SessionTokenError, SessionTokenVerifier, REFRESH_TOKEN_TTL,
    };
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use std::time::{Duration, SystemTime};

    #[tokio::test]
    async fn exchange_code_returns_a_signed_token_rather_than_an_opaque_uuid() {
        // Given a signed auth service
        let daemon = a_daemon_key(7);
        let bridge = signed_bridge("login-code", &daemon, &[&daemon]);
        let state = do_get_auth_url_state(&bridge).await;

        // When a code is exchanged
        let token = do_exchange(&bridge, "login-code", &state).await;

        // Then the returned token is a signed, self-describing token, not a bare UUID
        assert!(
            token.starts_with("v2."),
            "expected a signed token, got '{token}'"
        );
    }

    #[tokio::test]
    async fn exchange_code_fails_when_no_signer_is_configured() {
        // Given an auth service with no signing key
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let bridge = RpcBridge::new(AuthServiceServer::new(AuthServiceImpl::new(stub)));
        let state = do_get_auth_url_state(&bridge).await;

        // When a code is exchanged
        let exchange_req = ExchangeCodeRequest {
            code: "login-code".to_string(),
            state,
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&exchange_req),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        };
        let result = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await;

        // Then it is rejected — there is no key to mint a verifiable token with
        assert!(
            result.is_err(),
            "exchange must fail without a configured signer"
        );
    }

    #[tokio::test]
    async fn exchange_code_returns_both_an_access_token_and_a_refresh_token() {
        // Given a signed auth service that knows a login code
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let state = service
            .get_auth_url(Request::direct(GetAuthUrlRequest {}))
            .await
            .expect("auth url")
            .into_inner()
            .state;

        // When a code is exchanged
        let resp = service
            .exchange_code(Request::direct(ExchangeCodeRequest {
                code: "login-code".to_string(),
                state,
            }))
            .await
            .expect("exchange should succeed")
            .into_inner();

        // Then login returns an access token and a refresh token of the right kinds
        assert_eq!(
            (
                claims_under(&daemon, &resp.session_token).kind,
                claims_under(&daemon, &resp.refresh_token).kind
            ),
            (TokenKind::Access, TokenKind::Refresh)
        );
    }

    #[tokio::test]
    async fn refresh_session_mints_a_new_access_token_and_a_sliding_refresh_token() {
        // Given a signed service and a valid refresh token
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let refresh_token = a_signer_for(&daemon).mint_refresh(&user);

        // When the session is refreshed
        let resp = service
            .refresh_session(Request::direct(RefreshSessionRequest {
                refresh_token,
                ..Default::default()
            }))
            .await
            .expect("refresh of a valid refresh token should succeed")
            .into_inner();

        // Then it returns a new access token plus a refresh token slid to a fresh 7-day window
        let access = claims_under(&daemon, &resp.session_token);
        let refresh = claims_under(&daemon, &resp.refresh_token);
        assert_eq!(access.kind, TokenKind::Access);
        assert_eq!(refresh.kind, TokenKind::Refresh);
        assert_eq!(refresh.exp - refresh.iat, REFRESH_TOKEN_TTL.as_secs());
        assert_eq!(resp.user.expect("user").login, "testuser");
    }

    #[tokio::test]
    async fn refresh_session_rejects_an_access_kind_token() {
        // Given a signed service and an *access*-kind token
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let access_token = a_signer_for(&daemon).mint_access(&user);

        // When it is presented to refresh
        let result = service
            .refresh_session(Request::direct(RefreshSessionRequest {
                refresh_token: access_token,
                ..Default::default()
            }))
            .await;

        // Then it is rejected — a short-lived access token cannot extend a session
        assert!(
            result.is_err(),
            "an access-kind token must not be refreshable"
        );
    }

    #[tokio::test]
    async fn refresh_session_rejects_an_expired_refresh_token() {
        // Given a signed service and a refresh token whose 7-day window lapsed a day ago
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let expired = a_signer_for(&daemon).mint_kind_with_issued_at(
            &user,
            TokenKind::Refresh,
            SystemTime::now() - (REFRESH_TOKEN_TTL + Duration::from_secs(86_400)),
            REFRESH_TOKEN_TTL,
        );

        // When a refresh is attempted
        let result = service
            .refresh_session(Request::direct(RefreshSessionRequest {
                refresh_token: expired,
                ..Default::default()
            }))
            .await;

        // Then it is rejected — the session has ended and re-login is required
        assert!(
            result.is_err(),
            "refresh must reject an expired refresh token"
        );
    }

    /// A daemon that refuses every login, with the status it refuses with.
    struct RefusesEveryLogin;

    impl LoginAdmission for RefusesEveryLogin {
        fn admit(&self, github_login: &str, _transport: RequestTransport) -> Result<(), Status> {
            Err(Status::permission_denied(format!(
                "{github_login} is not admitted here"
            )))
        }
    }

    #[tokio::test]
    async fn a_login_the_daemon_does_not_admit_is_refused_with_the_daemons_reason() {
        // Given a signed auth service whose daemon admits nobody
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let service = a_signed_service(stub, &daemon, &[&daemon])
            .with_login_admission(Arc::new(RefusesEveryLogin));
        let bridge = RpcBridge::new(AuthServiceServer::new(service));
        let state = do_get_auth_url_state(&bridge).await;

        // When GitHub vouches for a login
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&ExchangeCodeRequest {
                code: "login-code".to_string(),
                state,
            }),
            metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
        };
        let refusal = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await
            .err();

        // Then no session is minted, and the client is told why
        assert_eq!(
            refusal.map(|status| (status.code, status.message)),
            Some((
                tddy_rpc::Code::PermissionDenied,
                "testuser is not admitted here".to_string()
            ))
        );
    }

    /// Records the transport of every login it is asked about, and admits each.
    #[derive(Default)]
    struct RecordsTransports(std::sync::Mutex<Vec<RequestTransport>>);

    impl LoginAdmission for RecordsTransports {
        fn admit(&self, _github_login: &str, transport: RequestTransport) -> Result<(), Status> {
            self.0.lock().unwrap().push(transport);
            Ok(())
        }
    }

    /// A signed service over a stub that knows `user` as `login-code`, asking `admission` about
    /// every completed login.
    fn a_bridge_admitting_through(
        admission: Arc<RecordsTransports>,
    ) -> RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>> {
        let daemon = a_daemon_key(9);
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let service = a_signed_service(stub, &daemon, &[&daemon]).with_login_admission(admission);
        RpcBridge::new(AuthServiceServer::new(service))
    }

    /// Call `method` with `request` as a host serving `transport` would hand it over.
    async fn call_over<Res: prost::Message + Default>(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        transport: RequestTransport,
        method: &str,
        request: impl prost::Message,
    ) -> Res {
        let message = tddy_rpc::RpcMessage::new(
            request.encode_to_vec(),
            tddy_rpc::RequestMetadata::over(transport),
        );
        match bridge
            .handle_messages("auth.AuthService", method, &[message])
            .await
            .unwrap_or_else(|status| panic!("{method} failed: {status:?}"))
        {
            tddy_rpc::ResponseBody::Complete(chunks) => {
                Res::decode(&chunks[0][..]).expect("a unary response decodes")
            }
            _ => panic!("{method} is unary"),
        }
    }

    #[tokio::test]
    async fn admission_is_told_the_transport_a_redirect_login_completed_over() {
        // Given a signed service whose admission records what it is told
        let admission = Arc::new(RecordsTransports::default());
        let bridge = a_bridge_admitting_through(Arc::clone(&admission));
        let state = do_get_auth_url_state(&bridge).await;

        // When the exchange arrives over the LiveKit room
        let _: ExchangeCodeResponse = call_over(
            &bridge,
            RequestTransport::LiveKit,
            "ExchangeCode",
            ExchangeCodeRequest {
                code: "login-code".to_string(),
                state,
            },
        )
        .await;

        // Then admission was told exactly that
        assert_eq!(
            *admission.0.lock().unwrap(),
            vec![RequestTransport::LiveKit]
        );
    }

    #[tokio::test]
    async fn admission_is_told_the_transport_a_device_login_completed_over() {
        // Given a signed service whose admission records what it is told, and a started device login
        let admission = Arc::new(RecordsTransports::default());
        let bridge = a_bridge_admitting_through(Arc::clone(&admission));
        let started: StartDeviceLoginResponse = call_over(
            &bridge,
            RequestTransport::InProcess,
            "StartDeviceLogin",
            StartDeviceLoginRequest {},
        )
        .await;

        // When it is polled to completion over a Unix socket
        poll_to_completion(&bridge, RequestTransport::UnixSocket, &started.device_code).await;

        // Then admission was told the transport of the poll that completed it
        assert_eq!(
            *admission.0.lock().unwrap(),
            vec![RequestTransport::UnixSocket]
        );
    }

    /// How many polls [`poll_to_completion`] makes before giving up. More than the stub answers
    /// `Pending` before approving, so only a stub that never approves exhausts it.
    const POLLS_BEFORE_GIVING_UP: usize = 5;

    /// Poll `device_code` over `transport` until it completes, failing — with every state seen —
    /// rather than spinning forever if it never does.
    async fn poll_to_completion(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        transport: RequestTransport,
        device_code: &str,
    ) {
        let mut seen = Vec::new();
        for _ in 0..POLLS_BEFORE_GIVING_UP {
            let polled: PollDeviceLoginResponse = call_over(
                bridge,
                transport,
                "PollDeviceLogin",
                PollDeviceLoginRequest {
                    device_code: device_code.to_string(),
                },
            )
            .await;
            seen.push(polled.state());
            if polled.state() == DeviceLoginState::Complete {
                return;
            }
        }
        panic!(
            "the device login never completed in {POLLS_BEFORE_GIVING_UP} polls; states seen: \
             {seen:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Every poll outcome that is not a completion reaches the client as its own proto state, with
    // an interval only when GitHub widened one.
    // -------------------------------------------------------------------------

    /// A provider whose every device-login poll answers `poll`. Nothing else is scripted: any
    /// other call fails, naming itself, so a test that strays off the poll path says so.
    struct ScriptedDeviceProvider {
        poll: DeviceLoginPoll,
    }

    impl ScriptedDeviceProvider {
        fn answering(poll: DeviceLoginPoll) -> Self {
            Self { poll }
        }
    }

    #[async_trait]
    impl GitHubOAuthProvider for ScriptedDeviceProvider {
        fn authorize_url(&self) -> Result<(String, String), String> {
            Err("ScriptedDeviceProvider scripts no authorize URL".to_string())
        }

        async fn exchange_code(
            &self,
            _code: &str,
            _state: &str,
        ) -> Result<(String, GitHubUser), String> {
            Err("ScriptedDeviceProvider scripts no code exchange".to_string())
        }

        async fn start_device_login(&self) -> Result<crate::provider::DeviceLoginStart, String> {
            Err("ScriptedDeviceProvider scripts no device-login start".to_string())
        }

        async fn poll_device_login(&self, _device_code: &str) -> Result<DeviceLoginPoll, String> {
            Ok(self.poll.clone())
        }

        fn issues_usable_access_token(&self) -> bool {
            false
        }
    }

    /// The `(state, interval_seconds)` a client is told when GitHub answers a poll with `poll`.
    async fn what_the_client_is_told_when_github_answers(
        poll: DeviceLoginPoll,
    ) -> (DeviceLoginState, u64) {
        let service = AuthServiceImpl::new(ScriptedDeviceProvider::answering(poll));
        let polled = AuthServiceTrait::poll_device_login(
            &service,
            Request::direct(PollDeviceLoginRequest {
                device_code: "the-device-code".to_string(),
            }),
        )
        .await
        .expect("a poll GitHub answered is not a failure")
        .into_inner();
        (polled.state(), polled.interval_seconds)
    }

    #[tokio::test]
    async fn a_slow_down_reaches_the_client_with_the_interval_to_obey() {
        // Given a GitHub that asks this daemon to slow down to ten seconds
        let answer = DeviceLoginPoll::SlowDown {
            interval_seconds: 10,
        };

        // When the client polls
        let told = what_the_client_is_told_when_github_answers(answer).await;

        // Then it is told to slow down, and by how much
        assert_eq!(told, (DeviceLoginState::SlowDown, 10));
    }

    #[tokio::test]
    async fn a_denial_reaches_the_client_as_denied_with_no_interval() {
        // Given a GitHub on which the operator refused the code
        let answer = DeviceLoginPoll::Denied;

        // When the client polls
        let told = what_the_client_is_told_when_github_answers(answer).await;

        // Then the attempt ends as denied, with no interval to wait out
        assert_eq!(told, (DeviceLoginState::Denied, 0));
    }

    #[tokio::test]
    async fn an_expiry_reaches_the_client_as_expired_with_no_interval() {
        // Given a GitHub on which the code outlived its window
        let answer = DeviceLoginPoll::Expired;

        // When the client polls
        let told = what_the_client_is_told_when_github_answers(answer).await;

        // Then the attempt ends as expired — not as denied — with no interval to wait out
        assert_eq!(told, (DeviceLoginState::Expired, 0));
    }
}
