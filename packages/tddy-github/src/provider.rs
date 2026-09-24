use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Domain model for a GitHub user (not proto — converted in auth_service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubUser {
    pub id: u64,
    pub login: String,
    pub avatar_url: String,
    pub name: String,
}

/// Trait abstracting GitHub OAuth operations. Implementations provide either
/// real GitHub API calls or an in-memory stub for testing.
#[async_trait]
pub trait GitHubOAuthProvider: Send + Sync + 'static {
    /// Generate the OAuth authorize URL and a CSRF state token.
    /// Returns (authorize_url, state).
    ///
    /// `Err` is a provider that cannot complete the redirect flow at all — a public client, which
    /// holds no secret to exchange the resulting code with. Handing out a URL the operator could
    /// follow to GitHub and back, only for the exchange to fail, is the thing this refuses.
    fn authorize_url(&self) -> Result<(String, String), String>;

    /// Exchange an authorization code for an access token and fetch user info.
    /// The state parameter must match one previously issued by authorize_url.
    /// Returns (access_token, user) on success.
    async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String>;

    /// Begin the OAuth **device flow** — the flow the GitHub CLI uses, and the only one that
    /// authenticates with a public `client_id` and no client secret.
    ///
    /// The daemon shows the returned [`DeviceLoginStart::user_code`] and sends the operator to
    /// [`DeviceLoginStart::verification_uri`]; approval happens in their browser, against GitHub,
    /// with nothing typed into this application.
    async fn start_device_login(&self) -> Result<DeviceLoginStart, String>;

    /// Ask GitHub whether the device code has been approved yet.
    ///
    /// Every outcome is a [`DeviceLoginPoll`] variant rather than an error string, because the
    /// caller must tell them apart: `Pending` is polled again, `SlowDown` widens the interval,
    /// and `Denied` and `Expired` each end the attempt for a different reason the operator is
    /// shown. A transport failure — GitHub unreachable, a response that does not parse — is the
    /// `Err` arm, and is the only thing in it.
    async fn poll_device_login(&self, device_code: &str) -> Result<DeviceLoginPoll, String>;

    /// Whether [`Self::exchange_code`]'s access token is a real GitHub credential that can
    /// authenticate API calls.
    ///
    /// `false` for the in-memory stub, whose token is synthetic — GitHub would reject it, so it must
    /// never be retained: a demo login holds no credential *by construction*, and its PR lookups
    /// resolve to a clean empty result rather than to an error (PR-stack UX recovery, D12).
    fn issues_usable_access_token(&self) -> bool;
}

/// What GitHub hands back when a device login begins.
///
/// `device_code` is the daemon's half and is never shown; `user_code` is the operator's half and
/// is the only part they type. `interval` is GitHub's *minimum* seconds between polls — polling
/// faster earns a `slow_down`, which is why it is carried rather than hard-coded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLoginStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    pub interval_seconds: u64,
}

/// Where one poll of a device login leaves the attempt.
///
/// Modelled as one enum rather than `Option` plus error strings because the caller's next move
/// differs for every variant, and three of the five are ordinary progress rather than failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceLoginPoll {
    /// The operator has not approved yet. Poll again after the interval.
    Pending,
    /// GitHub says the interval was too short. Widen it, then poll again.
    SlowDown { interval_seconds: u64 },
    /// The operator refused. The attempt is over and must not be retried silently.
    Denied,
    /// The device code outlived its window. A new one is needed.
    Expired,
    /// Approved. The access token and the user it belongs to, in the same shape
    /// [`GitHubOAuthProvider::exchange_code`] returns them.
    Complete {
        access_token: String,
        user: GitHubUser,
    },
}
