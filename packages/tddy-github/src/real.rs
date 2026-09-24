use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use uuid::Uuid;

use crate::provider::{DeviceLoginPoll, DeviceLoginStart, GitHubOAuthProvider, GitHubUser};

/// Real GitHub OAuth provider that calls GitHub's API endpoints.
pub struct RealGitHubProvider {
    client_id: String,
    /// What only the redirect flow needs. `None` for a **public client** (RFC 6749 §2.1): one
    /// that cannot keep a secret — a desktop application anyone can download — and so signs in by
    /// the device flow alone, with no callback to return to. Without it the redirect flow refuses
    /// rather than trying.
    redirect_client: Option<RedirectClient>,
    /// Where the OAuth endpoints live — `https://github.com` in production.
    oauth_base_url: String,
    /// Where the REST API lives — `https://api.github.com` in production.
    api_base_url: String,
    pending_states: Mutex<HashSet<String>>,
    /// Each device code this provider started and whose attempt has not ended, so a `slow_down`
    /// that names no interval of its own is widened from the one GitHub set for that code.
    ///
    /// Bounded: an entry leaves on a terminal answer, and every start and poll prunes the ones
    /// whose `expires_in` window has closed, so an attempt the operator abandoned does not stay.
    device_attempts: Mutex<HashMap<String, DeviceAttempt>>,
    http_client: reqwest::Client,
}

/// A confidential client's redirect-flow half: the secret its code exchange posts and the callback
/// its authorize URL names. Held together because neither means anything without the other.
struct RedirectClient {
    client_secret: String,
    redirect_uri: String,
}

/// What this provider remembers about a device code it started, for as long as GitHub could still
/// answer it with anything but `expired_token`.
struct DeviceAttempt {
    /// The minimum seconds between polls GitHub last set for this code.
    interval_seconds: u64,
    /// When the code's `expires_in` window closes.
    expires_at: Instant,
}

/// GitHub's own OAuth host. The default for [`RealGitHubProvider::new`].
pub const GITHUB_OAUTH_BASE_URL: &str = "https://github.com";

/// GitHub's own REST host. The default for [`RealGitHubProvider::new`].
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// The scopes every sign-in asks for, whichever flow carries it. `read:user` identifies the
/// operator; `repo` is what lets the granted token read (and later repoint/merge) pull requests on a
/// private repository — `read:user` alone cannot.
const SCOPES: &str = "read:user repo";

/// The `grant_type` that turns a device-flow poll into a token request (RFC 8628 §3.4).
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// How far to widen the interval on a `slow_down` that names none. This is protocol, not a
/// default: RFC 8628 §3.5 says a client told `slow_down` MUST increase its interval by 5 seconds
/// for this and all later requests, and GitHub's device-flow documentation says the same.
const SLOW_DOWN_WIDENING_SECONDS: u64 = 5;

/// Where, on the OAuth host, a code — a redirect-flow authorization code or a device code — is
/// exchanged for an access token.
const ACCESS_TOKEN_PATH: &str = "/login/oauth/access_token";

/// Where, on the OAuth host, a device-flow attempt is started.
const DEVICE_CODE_PATH: &str = "/login/device/code";

/// How one of the provider's POSTs words each way it can fail.
struct PostFailures {
    /// The prefix of a request that never got an answer.
    request: &'static str,
    /// The prefix of an answer whose status is not a success.
    status: &'static str,
    /// What the answer is, in "failed to parse {response}".
    response: &'static str,
}

const TOKEN_EXCHANGE_FAILURES: PostFailures = PostFailures {
    request: "token exchange request failed",
    status: "token exchange failed",
    response: "token response",
};

const DEVICE_CODE_FAILURES: PostFailures = PostFailures {
    request: "device code request failed",
    status: "device code request failed",
    response: "device code response",
};

const DEVICE_POLL_FAILURES: PostFailures = PostFailures {
    request: "device login poll failed",
    status: "device login poll failed",
    response: "device login poll response",
};

/// Why a public client refuses what only the redirect flow can do — `cannot` names that.
fn public_client_refusal(cannot: &str) -> String {
    format!(
        "this daemon holds a public client id and no client secret, so it cannot {cannot}; sign \
         in with the device flow"
    )
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// One answer to a device-flow poll. GitHub answers `200 OK` whether or not the code is approved
/// yet, so which of these fields is set is the whole of the outcome.
#[derive(Deserialize)]
struct DevicePollResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct GitHubApiUser {
    id: u64,
    login: String,
    avatar_url: String,
    name: Option<String>,
}

impl RealGitHubProvider {
    /// A confidential client: signs in by the redirect flow (and by the device flow too, which
    /// never sends the secret).
    pub fn new(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self::new_with_base_urls(
            client_id,
            client_secret,
            redirect_uri,
            GITHUB_OAUTH_BASE_URL,
            GITHUB_API_BASE_URL,
        )
    }

    /// The same provider pointed at other hosts.
    ///
    /// None of the provider's network code is reachable by a test while the hosts are string
    /// literals inside the request builders. A test serves both halves locally and passes their
    /// address here. It is a constructor rather than a cfg-gated branch precisely because it must
    /// be ordinary production code: GitHub Enterprise is the same substitution.
    pub fn new_with_base_urls(
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
        oauth_base_url: &str,
        api_base_url: &str,
    ) -> Self {
        Self::build(
            client_id,
            Some(RedirectClient {
                client_secret: client_secret.to_string(),
                redirect_uri: redirect_uri.to_string(),
            }),
            oauth_base_url,
            api_base_url,
        )
    }

    /// A **public client** — a `client_id` and no secret, the shape a desktop application ships
    /// in. It signs in by the device flow; its redirect-flow code exchange is refused, because
    /// that exchange cannot be made without a secret. It names no redirect URI, since the device
    /// flow returns to no callback.
    pub fn new_public(client_id: &str) -> Self {
        Self::new_public_with_base_urls(client_id, GITHUB_OAUTH_BASE_URL, GITHUB_API_BASE_URL)
    }

    /// [`Self::new_public`] pointed at other hosts — see [`Self::new_with_base_urls`].
    pub fn new_public_with_base_urls(
        client_id: &str,
        oauth_base_url: &str,
        api_base_url: &str,
    ) -> Self {
        Self::build(client_id, None, oauth_base_url, api_base_url)
    }

    fn build(
        client_id: &str,
        redirect_client: Option<RedirectClient>,
        oauth_base_url: &str,
        api_base_url: &str,
    ) -> Self {
        Self {
            client_id: client_id.to_string(),
            redirect_client,
            oauth_base_url: oauth_base_url.trim_end_matches('/').to_string(),
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            pending_states: Mutex::new(HashSet::new()),
            device_attempts: Mutex::new(HashMap::new()),
            http_client: reqwest::Client::new(),
        }
    }

    /// The GitHub user `access_token` belongs to. Shared by both flows: whichever way the token
    /// was granted, the operator is whoever GitHub says owns it.
    async fn fetch_user(&self, access_token: &str) -> Result<GitHubUser, String> {
        let user_resp = self
            .http_client
            .get(format!("{}/user", self.api_base_url))
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "tddy-github")
            .send()
            .await
            .map_err(|e| format!("user info request failed: {}", e))?;

        if !user_resp.status().is_success() {
            return Err(format!(
                "user info request failed with status: {}",
                user_resp.status()
            ));
        }

        let api_user: GitHubApiUser = user_resp
            .json()
            .await
            .map_err(|e| format!("failed to parse user response: {}", e))?;

        Ok(GitHubUser {
            id: api_user.id,
            login: api_user.login,
            avatar_url: api_user.avatar_url,
            name: api_user.name.unwrap_or_default(),
        })
    }

    /// POST `body` as JSON to `path` on the OAuth host, asking for a JSON answer, and parse that
    /// answer as `T`. Each failure is worded by `failures`.
    async fn post_for_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
        failures: &PostFailures,
    ) -> Result<T, String> {
        let resp = self
            .http_client
            .post(format!("{}{path}", self.oauth_base_url))
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| format!("{}: {}", failures.request, e))?;

        if !resp.status().is_success() {
            return Err(format!(
                "{} with status: {}",
                failures.status,
                resp.status()
            ));
        }

        resp.json()
            .await
            .map_err(|e| format!("failed to parse {}: {}", failures.response, e))
    }

    /// The device attempts still open, with every one whose window has closed dropped first.
    fn open_device_attempts(&self) -> std::sync::MutexGuard<'_, HashMap<String, DeviceAttempt>> {
        let mut attempts = self.device_attempts.lock().unwrap();
        let now = Instant::now();
        attempts.retain(|_, attempt| attempt.expires_at > now);
        attempts
    }

    /// The interval to obey after a `slow_down`: GitHub's own when it names one, otherwise the one
    /// GitHub last set for this device code widened by the RFC 8628 §3.5 step. Remembered for an
    /// open attempt, so a second `slow_down` widens from the first.
    ///
    /// A `slow_down` naming no interval, for a code this provider has no open attempt for — never
    /// started here, or past its window — is an error: there is no interval to widen, and
    /// inventing one would tell the client a number GitHub never said.
    fn widened_interval(&self, device_code: &str, from_github: Option<u64>) -> Result<u64, String> {
        let mut attempts = self.open_device_attempts();
        let attempt = attempts.get_mut(device_code);
        let widened = match (from_github, attempt) {
            (Some(interval), attempt) => {
                if let Some(attempt) = attempt {
                    attempt.interval_seconds = interval;
                }
                interval
            }
            (None, Some(attempt)) => {
                attempt.interval_seconds += SLOW_DOWN_WIDENING_SECONDS;
                attempt.interval_seconds
            }
            (None, None) => {
                return Err(format!(
                    "GitHub asked to slow down polling for a device code with no open attempt \
                     on this daemon ({device_code}), and named no interval to adopt"
                ))
            }
        };
        Ok(widened)
    }

    /// Forget a device code whose attempt has ended, whichever way it ended.
    fn forget_device_code(&self, device_code: &str) {
        self.open_device_attempts().remove(device_code);
    }

    /// What one device-flow poll's answer means, forgetting `device_code` once GitHub has ended
    /// its attempt. GitHub answers `200 OK` whether or not the code is approved yet, so this reads
    /// which of the answer's fields is set.
    async fn device_poll_outcome(
        &self,
        device_code: &str,
        polled: DevicePollResponse,
    ) -> Result<DeviceLoginPoll, String> {
        match (polled.access_token, polled.error.as_deref()) {
            (Some(access_token), None) => {
                self.forget_device_code(device_code);
                let user = self.fetch_user(&access_token).await?;
                Ok(DeviceLoginPoll::Complete { access_token, user })
            }
            (None, Some("authorization_pending")) => Ok(DeviceLoginPoll::Pending),
            (None, Some("slow_down")) => Ok(DeviceLoginPoll::SlowDown {
                interval_seconds: self.widened_interval(device_code, polled.interval)?,
            }),
            (None, Some("expired_token")) => {
                self.forget_device_code(device_code);
                Ok(DeviceLoginPoll::Expired)
            }
            (None, Some("access_denied")) => {
                self.forget_device_code(device_code);
                Ok(DeviceLoginPoll::Denied)
            }
            (_, Some(error)) => {
                self.forget_device_code(device_code);
                Err(format!(
                    "device login failed: {error}{}",
                    polled
                        .error_description
                        .map(|description| format!(": {description}"))
                        .unwrap_or_default()
                ))
            }
            (None, None) => Err(
                "failed to parse device login poll response: neither an access token nor an error"
                    .to_string(),
            ),
        }
    }
}

#[async_trait]
impl GitHubOAuthProvider for RealGitHubProvider {
    fn authorize_url(&self) -> Result<(String, String), String> {
        let Some(redirect_client) = self.redirect_client.as_ref() else {
            return Err(public_client_refusal("complete the redirect flow"));
        };
        let state = Uuid::new_v4().to_string();
        self.pending_states.lock().unwrap().insert(state.clone());
        // `SCOPES` is space-separated per OAuth; in a query string each space is `%20`.
        let url = format!(
            "{}/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&scope={}",
            self.oauth_base_url,
            self.client_id,
            redirect_client.redirect_uri,
            state,
            SCOPES.replace(' ', "%20")
        );
        Ok((url, state))
    }

    async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String> {
        // Before the state check: a public client issues no state, so every exchange it is asked
        // for would otherwise fail as a forged state rather than for the reason it really fails.
        let Some(RedirectClient { client_secret, .. }) = self.redirect_client.as_ref() else {
            return Err(public_client_refusal("exchange an authorization code"));
        };
        let state_valid = self.pending_states.lock().unwrap().remove(state);
        if !state_valid {
            return Err("invalid or expired state parameter".to_string());
        }

        let token_data: AccessTokenResponse = self
            .post_for_json(
                ACCESS_TOKEN_PATH,
                &serde_json::json!({
                    "client_id": self.client_id,
                    "client_secret": client_secret,
                    "code": code,
                }),
                &TOKEN_EXCHANGE_FAILURES,
            )
            .await?;

        let user = self.fetch_user(&token_data.access_token).await?;
        Ok((token_data.access_token, user))
    }

    async fn start_device_login(&self) -> Result<DeviceLoginStart, String> {
        // No client secret, on purpose and whichever kind of client this is: the device flow
        // authenticates with the public client id alone.
        let started: DeviceCodeResponse = self
            .post_for_json(
                DEVICE_CODE_PATH,
                &serde_json::json!({
                    "client_id": self.client_id,
                    "scope": SCOPES,
                }),
                &DEVICE_CODE_FAILURES,
            )
            .await?;

        self.open_device_attempts().insert(
            started.device_code.clone(),
            DeviceAttempt {
                interval_seconds: started.interval,
                expires_at: Instant::now() + Duration::from_secs(started.expires_in),
            },
        );
        Ok(DeviceLoginStart {
            device_code: started.device_code,
            user_code: started.user_code,
            verification_uri: started.verification_uri,
            expires_in_seconds: started.expires_in,
            interval_seconds: started.interval,
        })
    }

    async fn poll_device_login(&self, device_code: &str) -> Result<DeviceLoginPoll, String> {
        // Every poll prunes, so abandoned attempts leave even when nothing is ever started again.
        drop(self.open_device_attempts());
        let polled: DevicePollResponse = self
            .post_for_json(
                ACCESS_TOKEN_PATH,
                &serde_json::json!({
                    "client_id": self.client_id,
                    "device_code": device_code,
                    "grant_type": DEVICE_CODE_GRANT_TYPE,
                }),
                &DEVICE_POLL_FAILURES,
            )
            .await?;

        self.device_poll_outcome(device_code, polled).await
    }

    fn issues_usable_access_token(&self) -> bool {
        true
    }
}
