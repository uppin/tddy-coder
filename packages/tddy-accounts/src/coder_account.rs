//! Coder accounts: a [Coder](https://github.com/coder/coder) deployment's URL and a session token
//! for it, held in the vault beside the GitHub and model-provider accounts.
//!
//! A Coder account is an ordinary [`CredentialRecord`] — `provider` is [`CODER_PROVIDER`], `secret`
//! is the session token, and the deployment and the user the token belongs to live in `metadata` —
//! so it is sealed, listed, relabelled, removed and synced exactly like every other account. This
//! module is the one place that knows how a record spells one: [`CoderAccount::into_record`] and
//! [`CoderAccount::from_record`] are inverse, and nothing else reads those metadata keys.
//!
//! It is **not** a model-provider preset. A preset is a model endpoint with wire protocols and a
//! harness route; a deployment is neither, so [`ProviderAccount::from_record`] is `None` for a
//! Coder record and it never appears in a Provider field.
//!
//! The pair is checked before anything is stored, by asking the deployment who the token belongs
//! to ([`CoderDeploymentProbe`]). That is a port for the reason `ProviderModelProbe` is one: the
//! HTTP client is the daemon's, and this crate stays free of a network stack.
//!
//! [`ProviderAccount::from_record`]: crate::ProviderAccount::from_record

use std::sync::Arc;

use async_trait::async_trait;
use tddy_credentials::{AccountId, CredentialRecord, SecretString, SessionVaults};
use tddy_rpc::Status;
use tddy_service::proto::accounts::ConnectCoderAccountRequest;

use crate::service::AccountsServiceImpl;
use crate::store::AccountsError;
use crate::vault_store::SessionSubjectResolver;

/// The `provider` every Coder account's vault record carries, and the group it lists under.
pub const CODER_PROVIDER: &str = "coder";

/// The root of a Coder deployment, e.g. `https://coder.example.com`.
///
/// An `http(s)` URL with a host and nothing that would smuggle a credential or reshape the request
/// (userinfo, query, fragment, whitespace) — the rules `ConnectProviderAccount` applies to an
/// operator-supplied root. Held without a trailing slash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderDeploymentUrl(String);

impl CoderDeploymentUrl {
    /// `input` as a deployment root, or [`CoderAccountError::InvalidUrl`] naming it.
    pub fn parse(input: &str) -> Result<Self, CoderAccountError> {
        // TODO(coder-account): implement — validate, and drop the trailing slash.
        Ok(Self(input.to_string()))
    }

    /// The root as written to the vault and handed to the `coder` CLI, without a trailing slash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The authority's host, with its port when the URL names one — what an account id ends in.
    #[must_use]
    pub fn host(&self) -> &str {
        // TODO(coder-account): implement
        ""
    }
}

/// Who a session token belongs to, as the deployment's `GET /api/v2/users/me` answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderUser {
    /// The deployment's immutable id for the user (a UUID). The dedup key: a rename keeps it.
    pub id: String,
    /// The login name, shown as the account's subject.
    pub username: String,
}

/// Why a deployment did not say who a token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoderProbeError {
    /// The deployment answered and refused the token. Carries the deployment's own words.
    Rejected(String),
    /// Nothing answered at the URL: DNS, connect, TLS or a timeout. Carries the reason.
    Unreachable(String),
    /// Something answered, but not as a Coder deployment does — any other status, or a body that
    /// names no user. Carries what was seen.
    Unusable(String),
}

/// Asks a Coder deployment who a session token belongs to.
///
/// A port, because the HTTP client is the daemon's. The same call is the connect-time check that
/// the URL and the token belong together.
#[async_trait]
pub trait CoderDeploymentProbe: Send + Sync {
    /// The user `token` authenticates as at `url`.
    async fn whoami(
        &self,
        url: &CoderDeploymentUrl,
        token: &SecretString,
    ) -> Result<CoderUser, CoderProbeError>;
}

/// A Coder account, decoded from the vault record that holds it. Carries no token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderAccount {
    /// `coder-<user id>@<deployment host>`: one account per user per deployment, stable across a
    /// reconnect and a rename of the user.
    pub account_id: AccountId,
    pub label: String,
    pub deployment_url: CoderDeploymentUrl,
    pub username: String,
    pub user_id: String,
}

impl CoderAccount {
    /// The account `user` has at `deployment_url`, labelled `label`.
    #[must_use]
    pub fn for_user(deployment_url: CoderDeploymentUrl, user: &CoderUser, label: &str) -> Self {
        // TODO(coder-account): implement — the id is `coder-<user id>@<host>`.
        let _ = (user, label);
        Self {
            account_id: AccountId::new(""),
            label: String::new(),
            deployment_url,
            username: String::new(),
            user_id: String::new(),
        }
    }

    /// The Coder account `record` holds, or `None` when it holds some other kind of account.
    #[must_use]
    pub fn from_record(record: &CredentialRecord) -> Option<Self> {
        // TODO(coder-account): implement
        let _ = record;
        None
    }

    /// The vault record that holds this account with `token`, written at `now` (seconds since the
    /// epoch) — which is also when the token was last shown to work, since an account is only
    /// ever written straight after its deployment named the user.
    #[must_use]
    pub fn into_record(self, token: SecretString, now: u64) -> CredentialRecord {
        // TODO(coder-account): implement — provider, secret, and the `url`, `subject`,
        // `subject_id` and `probed_at` metadata.
        let _ = (token, now);
        CredentialRecord {
            provider: tddy_credentials::ProviderId::new(""),
            account: self.account_id,
            label: String::new(),
            secret: SecretString::new(""),
            metadata: std::collections::BTreeMap::new(),
            updated_at: 0,
            version: tddy_credentials::FIRST_VERSION,
        }
    }
}

/// What a session on a Coder workspace needs from an account: where the deployment is, and the
/// token to present there.
///
/// Readable only while the owner's vault is open, and never written anywhere but the vault: a
/// caller that needs it for a running session holds it in memory for that session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderCredentials {
    pub url: CoderDeploymentUrl,
    pub token: SecretString,
}

/// Why a Coder account was not connected, or its credentials not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoderAccountError {
    /// The deployment URL is not an `http(s)` URL with a host and without userinfo, query or
    /// fragment. Carries the URL as it was given.
    InvalidUrl(String),
    /// No session token was given.
    MissingToken,
    /// The deployment refused the token. Carries the deployment's own words.
    Rejected(String),
    /// The deployment could not be asked, or did not answer as one. Carries the reason.
    Unreachable(String),
    /// The vault is open and holds no Coder account by this id.
    NotFound(AccountId),
    /// The vault refused: no such session, locked, uninitialized, or unreadable.
    Vault(AccountsError),
}

/// Coder accounts in the vaults a daemon holds: connecting one, and reading one's credentials.
pub struct CoderAccounts {
    // TODO(coder-account): implement — the three ports are read by `connect` and `credentials_for`.
    #[allow(dead_code)]
    vaults: Arc<SessionVaults>,
    #[allow(dead_code)]
    subject_of: SessionSubjectResolver,
    #[allow(dead_code)]
    probe: Arc<dyn CoderDeploymentProbe>,
}

impl CoderAccounts {
    /// Over the daemon's open vaults, the resolver that names a session token's subject, and the
    /// way a deployment is asked who a token belongs to.
    #[must_use]
    pub fn new(
        vaults: Arc<SessionVaults>,
        subject_of: SessionSubjectResolver,
        probe: Arc<dyn CoderDeploymentProbe>,
    ) -> Self {
        Self {
            vaults,
            subject_of,
            probe,
        }
    }

    /// Check `token` against the deployment at `deployment_url` and seal the pair in the session
    /// owner's vault. The record as written.
    ///
    /// The vault is consulted before the network, so a closed vault refuses without the token
    /// leaving this daemon. Connecting a deployment and user the vault already holds replaces that
    /// record's token and keeps its account id and label.
    pub async fn connect(
        &self,
        session_token: &str,
        deployment_url: &str,
        token: SecretString,
        label: &str,
    ) -> Result<CredentialRecord, CoderAccountError> {
        // TODO(coder-account): implement
        let _ = (session_token, deployment_url, token, label);
        Err(CoderAccountError::Vault(AccountsError::Unavailable(
            "connecting a Coder account is not implemented on this daemon yet".to_string(),
        )))
    }

    /// The deployment URL and token of `subject`'s Coder account `account_id`.
    ///
    /// By subject rather than by session token: the caller is a session being started or run for a
    /// person the daemon has already identified. A vault that is not open is
    /// [`CoderAccountError::Vault`], never an empty answer.
    pub fn credentials_for(
        &self,
        subject: &str,
        account_id: &AccountId,
    ) -> Result<CoderCredentials, CoderAccountError> {
        // TODO(coder-account): implement
        let _ = (subject, account_id);
        Err(CoderAccountError::Vault(AccountsError::Unavailable(
            "reading a Coder account's credentials is not implemented on this daemon yet"
                .to_string(),
        )))
    }
}

impl<S> AccountsServiceImpl<S> {
    /// Wire the half that connects Coder deployments.
    ///
    /// Additive, like `with_provider_accounts`: a daemon that skips it answers `ConnectCoderAccount`
    /// with `FAILED_PRECONDITION` and nothing else changes.
    #[must_use]
    pub fn with_coder_accounts(mut self, coder: Arc<CoderAccounts>) -> Self {
        self.coder = Some(coder);
        self
    }
}

/// What `ConnectCoderAccount` answers: the stored record, or the status its refusal becomes —
/// `INVALID_ARGUMENT` for the URL or an empty token, `UNAUTHENTICATED` when the deployment rejects
/// the token, `UNAVAILABLE` when it cannot be reached, `FAILED_PRECONDITION` for a closed vault or
/// a daemon that wired no Coder accounts.
pub(crate) async fn connect_coder_account(
    coder: Option<&CoderAccounts>,
    request: ConnectCoderAccountRequest,
) -> Result<CredentialRecord, Status> {
    // TODO(coder-account): implement
    let _ = (coder, request);
    Err(Status::unimplemented(
        "connecting a Coder account is not implemented on this daemon yet",
    ))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_credentials::ProviderId;

    use super::*;

    const A_DEPLOYMENT: &str = "https://coder.example.com";
    const A_TOKEN: &str = "a-coder-session-token";
    const CONNECTED_AT: u64 = 1_760_000_000;

    fn a_deployment_url() -> CoderDeploymentUrl {
        CoderDeploymentUrl::parse(A_DEPLOYMENT).expect("a deployment URL")
    }

    fn ada_at_the_deployment() -> CoderUser {
        CoderUser {
            id: "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10".to_string(),
            username: "ada-lovelace".to_string(),
        }
    }

    fn adas_account() -> CoderAccount {
        CoderAccount {
            account_id: AccountId::new(
                "coder-3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10@coder.example.com",
            ),
            label: "Work Coder".to_string(),
            deployment_url: a_deployment_url(),
            username: "ada-lovelace".to_string(),
            user_id: "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10".to_string(),
        }
    }

    fn a_token() -> SecretString {
        SecretString::new(A_TOKEN)
    }

    #[test]
    fn a_deployment_url_is_held_without_its_trailing_slash() {
        // When
        let url = CoderDeploymentUrl::parse("https://coder.example.com/").expect("a valid URL");

        // Then
        assert_eq!(url.as_str(), "https://coder.example.com");
    }

    #[test]
    fn a_deployment_urls_host_keeps_its_port() {
        // When
        let url = CoderDeploymentUrl::parse("http://coder.internal:3000").expect("a valid URL");

        // Then
        assert_eq!(url.host(), "coder.internal:3000");
    }

    #[test]
    fn a_url_carrying_userinfo_is_refused() {
        // Given
        let url = "https://ada:hunter2@coder.example.com";

        // When
        let parsed = CoderDeploymentUrl::parse(url);

        // Then
        assert_eq!(parsed, Err(CoderAccountError::InvalidUrl(url.to_string())));
    }

    #[test]
    fn a_url_carrying_a_query_is_refused() {
        // Given
        let url = "https://coder.example.com?token=abc";

        // When
        let parsed = CoderDeploymentUrl::parse(url);

        // Then
        assert_eq!(parsed, Err(CoderAccountError::InvalidUrl(url.to_string())));
    }

    #[test]
    fn a_url_carrying_a_fragment_is_refused() {
        // Given
        let url = "https://coder.example.com#workspaces";

        // When
        let parsed = CoderDeploymentUrl::parse(url);

        // Then
        assert_eq!(parsed, Err(CoderAccountError::InvalidUrl(url.to_string())));
    }

    #[test]
    fn a_url_that_is_not_http_or_https_is_refused() {
        // Given
        let url = "ssh://coder.example.com";

        // When
        let parsed = CoderDeploymentUrl::parse(url);

        // Then
        assert_eq!(parsed, Err(CoderAccountError::InvalidUrl(url.to_string())));
    }

    #[test]
    fn a_url_without_a_host_is_refused() {
        // Given
        let url = "https:///workspaces";

        // When
        let parsed = CoderDeploymentUrl::parse(url);

        // Then
        assert_eq!(parsed, Err(CoderAccountError::InvalidUrl(url.to_string())));
    }

    #[test]
    fn an_account_is_identified_by_the_user_id_and_the_deployment_host() {
        // When
        let account =
            CoderAccount::for_user(a_deployment_url(), &ada_at_the_deployment(), "Work Coder");

        // Then
        assert_eq!(account, adas_account());
    }

    #[test]
    fn the_record_is_a_coder_record_sealing_the_token_and_naming_the_user() {
        // When
        let record = adas_account().into_record(a_token(), CONNECTED_AT);

        // Then
        assert_eq!(record.provider, ProviderId::new(CODER_PROVIDER));
        assert_eq!(record.account, adas_account().account_id);
        assert_eq!(record.label, "Work Coder");
        assert_eq!(record.secret, a_token());
        assert_eq!(record.updated_at, CONNECTED_AT);
        assert_eq!(
            record.metadata,
            std::collections::BTreeMap::from([
                ("url".to_string(), A_DEPLOYMENT.to_string()),
                ("subject".to_string(), "ada-lovelace".to_string()),
                (
                    "subject_id".to_string(),
                    "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10".to_string()
                ),
                ("probed_at".to_string(), CONNECTED_AT.to_string()),
            ])
        );
    }

    #[test]
    fn a_coder_account_survives_a_round_trip_through_its_record() {
        // Given
        let record = adas_account().into_record(a_token(), CONNECTED_AT);

        // When
        let decoded = CoderAccount::from_record(&record);

        // Then
        assert_eq!(decoded, Some(adas_account()));
    }

    #[test]
    fn a_record_that_names_no_deployment_is_not_a_coder_account() {
        // Given a record under the coder provider whose metadata lost its URL
        let whole = adas_account().into_record(a_token(), CONNECTED_AT);
        let mut without_a_url = whole.clone();
        without_a_url.metadata.remove("url");

        // When
        let decoded = (
            CoderAccount::from_record(&whole).is_some(),
            CoderAccount::from_record(&without_a_url).is_some(),
        );

        // Then
        assert_eq!(decoded, (true, false));
    }
}
