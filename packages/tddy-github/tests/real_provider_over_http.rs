//! [`RealGitHubProvider`] against a GitHub that answers from `127.0.0.1`.
//!
//! Every branch in this provider is an HTTP outcome, and until now none of them had a test: the
//! endpoints were string literals inside the request builders, so the only way to reach a failure
//! arm was to break the real GitHub. `new_with_base_urls` is the seam that fixes it — the same
//! substitution GitHub Enterprise needs, so it is ordinary production code rather than a test-only
//! branch.
//!
//! The device flow is the reason it exists now. It doubles this crate's network surface and adds
//! states — `authorization_pending`, `slow_down`, `access_denied`, `expired_token` — that are
//! *not* errors and must not be handled as such, and no amount of care in the implementation
//! substitutes for exercising them.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use tddy_github::provider::{DeviceLoginPoll, GitHubOAuthProvider};
use tddy_github::RealGitHubProvider;

const THE_CLIENT_ID: &str = "Iv1.0123456789abcdef";
const THE_SECRET: &str = "a-secret-no-device-flow-may-send";

#[tokio::test]
async fn a_started_device_login_carries_githubs_own_codes_and_interval() {
    // Given a GitHub that issues a device code
    let github = a_github_answering(vec![Answer::DeviceCode {
        user_code: "WDJB-MJHT".to_string(),
        interval: 5,
    }])
    .await;

    // When a device login begins
    let started = the_provider(&github)
        .start_device_login()
        .await
        .expect("GitHub issued a device code");

    // Then what GitHub said is what the operator is shown, interval included
    assert_eq!(
        (started.user_code.as_str(), started.interval_seconds),
        ("WDJB-MJHT", 5)
    );
}

#[tokio::test]
async fn a_slow_down_hands_back_the_wider_interval_to_obey() {
    // Given a GitHub that has decided this daemon is polling too fast
    let github = a_github_answering(vec![Answer::PollError {
        error: "slow_down".to_string(),
        interval: Some(10),
    }])
    .await;

    // When the device login is polled
    let polled = the_provider(&github)
        .poll_device_login("the-device-code")
        .await
        .expect("a slow_down is an answer, not a transport failure");

    // Then the wider interval comes back, rather than a failure the caller would give up on
    assert_eq!(
        polled,
        DeviceLoginPoll::SlowDown {
            interval_seconds: 10
        }
    );
}

#[tokio::test]
async fn a_refusal_and_an_expiry_are_told_apart() {
    // Given a GitHub that refuses one attempt and expires another
    let refusing = a_github_answering(vec![Answer::PollError {
        error: "access_denied".to_string(),
        interval: None,
    }])
    .await;
    let expiring = a_github_answering(vec![Answer::PollError {
        error: "expired_token".to_string(),
        interval: None,
    }])
    .await;

    // When each is polled
    let refused = the_provider(&refusing).poll_device_login("d").await;
    let expired = the_provider(&expiring).poll_device_login("d").await;

    // Then they are distinct outcomes — one the operator chose, one the clock caused
    assert_eq!(
        (refused, expired),
        (Ok(DeviceLoginPoll::Denied), Ok(DeviceLoginPoll::Expired))
    );
}

#[tokio::test]
async fn an_unapproved_device_login_is_pending_rather_than_failed() {
    // Given a GitHub waiting on the operator
    let github = a_github_answering(vec![Answer::PollError {
        error: "authorization_pending".to_string(),
        interval: None,
    }])
    .await;

    // When the device login is polled
    let polled = the_provider(&github).poll_device_login("d").await;

    // Then it is ordinary progress
    assert_eq!(polled, Ok(DeviceLoginPoll::Pending));
}

#[tokio::test]
async fn no_request_in_the_device_flow_carries_the_client_secret() {
    // Given a provider that holds a client secret, and a GitHub recording everything it receives
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::AccessToken("gho_a-granted-token".to_string()),
        Answer::User {
            login: "operator".to_string(),
        },
    ])
    .await;
    let provider = the_provider(&github);

    // When a whole device login is started and polled through to its completion
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then every leg was asked for, and the secret travelled in no path, query, header or body of
    // any of them — the flow exists precisely so it need not
    assert_eq!(
        (
            polled.map(|poll| matches!(poll, DeviceLoginPoll::Complete { .. })),
            github.requested_endpoints(),
            github.request_parts_containing(THE_SECRET),
        ),
        (
            Ok(true),
            THE_DEVICE_FLOW_ENDPOINTS.map(str::to_string).to_vec(),
            Vec::<String>::new(),
        ),
        "the device flow authenticates with a public client id alone; requests were {:?}",
        github.received.lock().unwrap()
    );
}

#[tokio::test]
async fn a_code_exchange_refuses_a_state_it_never_issued() {
    // Given a provider that has issued no authorize URL
    let github = a_github_answering(vec![]).await;

    // When a callback arrives quoting a state from nowhere
    let exchanged = the_provider(&github)
        .exchange_code("c", "forged-state")
        .await;

    // Then it is refused before a single byte goes to GitHub
    assert_eq!(
        (exchanged.map(|_| ()), github.received.lock().unwrap().len()),
        (Err("invalid or expired state parameter".to_string()), 0)
    );
}

#[tokio::test]
async fn a_code_exchange_reports_which_leg_of_the_conversation_failed() {
    // Given three GitHubs, each breaking a different leg of the exchange
    let rejecting_token = a_github_answering(vec![Answer::Status(401)]).await;
    let garbling_token = a_github_answering(vec![Answer::Garbage]).await;
    let rejecting_user = a_github_answering(vec![
        Answer::AccessToken("gho_a-granted-token".to_string()),
        Answer::Status(403),
    ])
    .await;
    let garbling_user = a_github_answering(vec![
        Answer::AccessToken("gho_a-granted-token".to_string()),
        Answer::Garbage,
    ])
    .await;
    let unreachable = a_github_nobody_is_answering();

    // When a code is exchanged against each
    let outcomes: Vec<Result<(), String>> = vec![
        exchange_against(&rejecting_token).await,
        exchange_against(&garbling_token).await,
        exchange_against(&rejecting_user).await,
        exchange_against(&garbling_user).await,
        exchange_against(&unreachable).await,
    ];

    // Then each failure names its own leg, rather than collapsing into one opaque error
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.as_ref().unwrap_err().as_str())
            .map(|message| message.split(':').next().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "token exchange failed with status",
            "failed to parse token response",
            "user info request failed with status",
            "failed to parse user response",
            "token exchange request failed",
        ]
    );
}

#[tokio::test]
async fn a_code_exchange_returns_the_granted_token_and_the_user_it_belongs_to() {
    // Given a GitHub that grants a token and knows the user
    let github = a_github_answering(vec![
        Answer::AccessToken("gho_a-granted-token".to_string()),
        Answer::User {
            login: "operator".to_string(),
        },
    ])
    .await;

    // When a code is exchanged
    let provider = the_provider(&github);
    let (_, state) = provider
        .authorize_url()
        .expect("a confidential client issues an authorize URL");
    let exchanged = provider.exchange_code("the-code", &state).await;

    // Then both halves come back
    assert_eq!(
        exchanged.map(|(token, user)| (token, user.login)),
        Ok(("gho_a-granted-token".to_string(), "operator".to_string()))
    );
}

#[tokio::test]
async fn a_code_exchange_whose_token_was_granted_but_whose_user_is_unreachable_names_the_user_leg()
{
    // Given a GitHub that grants a token, and a REST API nobody is answering
    let oauth =
        a_github_answering(vec![Answer::AccessToken("gho_a-granted-token".to_string())]).await;
    let api = a_github_nobody_is_answering();

    // When a code is exchanged
    let provider = a_provider_split_across(&oauth, &api);
    let (_, state) = provider
        .authorize_url()
        .expect("a confidential client issues an authorize URL");
    let exchanged = provider.exchange_code("the-code", &state).await;

    // Then the failure names the user leg, not the token leg that succeeded
    assert_eq!(
        exchanged
            .map(|_| ())
            .unwrap_err()
            .split(':')
            .next()
            .map(str::to_string),
        Some("user info request failed".to_string())
    );
}

#[tokio::test]
async fn a_slow_down_naming_no_interval_widens_the_one_github_set_by_five_seconds() {
    // Given a device login GitHub started at a 5-second interval, then asks to slow down without
    // saying by how much
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");

    // When the device login is polled
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then the interval is widened by the step GitHub documents
    assert_eq!(
        polled,
        Ok(DeviceLoginPoll::SlowDown {
            interval_seconds: 10
        })
    );
}

#[tokio::test]
async fn a_public_client_refuses_a_code_exchange_before_asking_github() {
    // Given a provider holding a client id and no secret
    let github = a_github_answering(vec![]).await;
    let provider = a_public_client(&github);

    // When a redirect-flow callback is exchanged
    let exchanged = provider.exchange_code("the-code", "a-state").await;

    // Then it is refused for want of a secret, naming the flow that works, without a request
    assert_eq!(
        (
            exchanged
                .map(|_| ())
                .is_err_and(|e| e.contains("device flow")),
            github.received.lock().unwrap().len()
        ),
        (true, 0)
    );
}

#[tokio::test]
async fn a_public_client_hands_out_no_authorize_url_it_could_never_complete() {
    // Given a provider holding a client id and no secret
    let github = a_github_answering(vec![]).await;
    let provider = a_public_client(&github);

    // When the redirect flow is begun
    let authorize = provider.authorize_url();

    // Then it is refused, naming the flow that works, rather than sending the operator to GitHub
    // for a code this provider cannot exchange
    assert!(
        authorize.as_ref().is_err_and(|e| e.contains("device flow")),
        "a public client must not begin the redirect flow; got {authorize:?}"
    );
}

#[tokio::test]
async fn a_slow_down_naming_no_interval_for_a_device_code_never_started_is_an_error() {
    // Given a GitHub asking to slow down without saying by how much
    let github = a_github_answering(vec![Answer::PollError {
        error: "slow_down".to_string(),
        interval: None,
    }])
    .await;

    // When a device code this provider never started is polled
    let polled = the_provider(&github)
        .poll_device_login("a-code-from-nowhere")
        .await;

    // Then there is no interval to widen, and no invented one is widened instead
    assert_refused_for_want_of_an_open_attempt(&polled);
}

#[tokio::test]
async fn a_device_code_is_forgotten_once_its_window_has_passed() {
    // Given a device login GitHub started with a window that has already closed, which is later
    // asked to slow down without an interval
    let github = a_github_answering(vec![
        Answer::DeviceCodeExpiringIn {
            expires_in: 0,
            interval: 5,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");

    // When the abandoned code is polled after its window
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then its interval was dropped with it, so nothing remains to widen from
    assert_refused_for_want_of_an_open_attempt(&polled);
}

#[tokio::test]
async fn a_device_code_is_forgotten_once_github_answers_it_expired() {
    // Given a started device login that GitHub reports expired, and is then asked to slow down
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::PollError {
            error: "expired_token".to_string(),
            interval: None,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");
    let expired = provider.poll_device_login(&started.device_code).await;

    // When the same code is polled again
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then the expiry ended the entry, so a later slow_down finds nothing to widen
    assert_eq!(expired, Ok(DeviceLoginPoll::Expired));
    assert_refused_for_want_of_an_open_attempt(&polled);
}

#[tokio::test]
async fn a_second_slow_down_widens_from_the_first() {
    // Given a device login GitHub started at a 5-second interval, then asks twice to slow down
    // without saying by how much
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");

    // When the device login is polled twice
    let first = provider.poll_device_login(&started.device_code).await;
    let second = provider.poll_device_login(&started.device_code).await;

    // Then each slow_down widens the interval the last one set — 5, then 10, then 15 — as RFC 8628
    // §3.5 requires "for this and all subsequent requests"
    assert_eq!(
        (first, second),
        (
            Ok(DeviceLoginPoll::SlowDown {
                interval_seconds: 10
            }),
            Ok(DeviceLoginPoll::SlowDown {
                interval_seconds: 15
            })
        )
    );
}

#[tokio::test]
async fn an_unknown_device_error_fails_and_forgets_the_code() {
    // Given a started device login GitHub answers with an error the device flow does not define,
    // and is then asked to slow down without an interval
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::PollError {
            error: "incorrect_device_code".to_string(),
            interval: None,
        },
        Answer::PollError {
            error: "slow_down".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");

    // When the code is polled, and polled again
    let failed = provider.poll_device_login(&started.device_code).await;
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then the unknown error fails the attempt by name, and ended its entry, so the later
    // slow_down finds nothing to widen
    assert_eq!(
        failed,
        Err("device login failed: incorrect_device_code".to_string())
    );
    assert_refused_for_want_of_an_open_attempt(&polled);
}

#[tokio::test]
async fn a_public_client_signs_in_by_the_device_flow() {
    // Given a provider holding a client id and no secret, and a GitHub that approves at once
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::AccessToken("gho_a-granted-token".to_string()),
        Answer::User {
            login: "operator".to_string(),
        },
    ])
    .await;
    let provider = a_public_client(&github);

    // When a device login is started and polled
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");
    let polled = provider.poll_device_login(&started.device_code).await;

    // Then it completes with the token GitHub granted and the user it belongs to, having asked
    // for a device code, a token for it and the token's user — in that order
    assert_eq!(
        (
            polled.map(|poll| match poll {
                DeviceLoginPoll::Complete { access_token, user } =>
                    Some((access_token, user.login)),
                _ => None,
            }),
            github.requested_endpoints(),
        ),
        (
            Ok(Some((
                "gho_a-granted-token".to_string(),
                "operator".to_string()
            ))),
            THE_DEVICE_FLOW_ENDPOINTS.map(str::to_string).to_vec(),
        )
    );
}

/// A device login's three legs, in order: a code, a token for the code, and the token's user.
const THE_DEVICE_FLOW_ENDPOINTS: [&str; 3] =
    ["/login/device/code", "/login/oauth/access_token", "/user"];

/// What `poll_device_login` begins its refusal with when a `slow_down` names no interval and there
/// is no open attempt to widen one from. Only the prefix: the rest quotes the device code.
const NO_OPEN_ATTEMPT_TO_WIDEN: &str =
    "GitHub asked to slow down polling for a device code with no open attempt on this daemon";

/// Assert the poll was refused because no open attempt held an interval to widen — not merely
/// that it failed, which a transport error or an unparseable answer would also do.
fn assert_refused_for_want_of_an_open_attempt(polled: &Result<DeviceLoginPoll, String>) {
    assert_eq!(
        polled
            .as_ref()
            .map_err(|refusal| refusal.get(..NO_OPEN_ATTEMPT_TO_WIDEN.len())),
        Err(Some(NO_OPEN_ATTEMPT_TO_WIDEN)),
        "expected a refusal for want of an open attempt; got {polled:?}"
    );
}

/// Run one exchange against a GitHub, through a state that provider actually issued.
async fn exchange_against(github: &AGitHub) -> Result<(), String> {
    let provider = the_provider(github);
    let (_, state) = provider
        .authorize_url()
        .expect("a confidential client issues an authorize URL");
    provider.exchange_code("the-code", &state).await.map(|_| ())
}

/// A port nothing is listening on, so the very first request fails in transport.
fn a_github_nobody_is_answering() -> AGitHub {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback has a free port");
    let address = listener.local_addr().expect("the listener has an address");
    drop(listener);
    AGitHub {
        address,
        received: Arc::new(Mutex::new(Vec::new())),
    }
}

fn the_provider(github: &AGitHub) -> RealGitHubProvider {
    let base = format!("http://{}", github.address);
    RealGitHubProvider::new_with_base_urls(
        THE_CLIENT_ID,
        THE_SECRET,
        "http://127.0.0.1/auth/callback",
        &base,
        &base,
    )
}

/// A confidential client whose OAuth host is `oauth` and whose REST API is `api`.
fn a_provider_split_across(oauth: &AGitHub, api: &AGitHub) -> RealGitHubProvider {
    RealGitHubProvider::new_with_base_urls(
        THE_CLIENT_ID,
        THE_SECRET,
        "http://127.0.0.1/auth/callback",
        &format!("http://{}", oauth.address),
        &format!("http://{}", api.address),
    )
}

/// A public client — the desktop shape: a client id and no secret at all.
fn a_public_client(github: &AGitHub) -> RealGitHubProvider {
    let base = format!("http://{}", github.address);
    RealGitHubProvider::new_public_with_base_urls(THE_CLIENT_ID, &base, &base)
}

/// One canned reply, in the order the provider will ask for them.
#[derive(Clone)]
enum Answer {
    DeviceCode {
        user_code: String,
        interval: u64,
    },
    /// A device code whose window is `expires_in` seconds, not the usual fifteen minutes.
    DeviceCodeExpiringIn {
        expires_in: u64,
        interval: u64,
    },
    PollError {
        error: String,
        interval: Option<u64>,
    },
    AccessToken(String),
    User {
        login: String,
    },
    Status(u16),
    Garbage,
}

/// A GitHub that lives on loopback for the length of one test, answering in a fixed order and
/// keeping every request it was sent — where it went, its headers and its body.
struct AGitHub {
    address: SocketAddr,
    received: Arc<Mutex<Vec<AReceivedRequest>>>,
}

/// One request as GitHub saw it, whole: nothing a client sends can live anywhere but here.
#[derive(Debug)]
struct AReceivedRequest {
    path_and_query: String,
    /// Each header as `name: value`.
    headers: Vec<String>,
    body: String,
}

impl AReceivedRequest {
    /// Every part of this request a secret could have travelled in.
    fn parts(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.path_and_query.as_str())
            .chain(self.headers.iter().map(String::as_str))
            .chain(std::iter::once(self.body.as_str()))
    }
}

impl AGitHub {
    /// The endpoints asked for, in the order they were asked.
    fn requested_endpoints(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.path_and_query.clone())
            .collect()
    }

    /// Every part of every request received that contains `needle`.
    fn request_parts_containing(&self, needle: &str) -> Vec<String> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .flat_map(|request| {
                request
                    .parts()
                    .filter(|part| part.contains(needle))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

#[derive(Clone)]
struct TheDesk {
    answers: Arc<Mutex<Vec<Answer>>>,
    received: Arc<Mutex<Vec<AReceivedRequest>>>,
}

async fn a_github_answering(answers: Vec<Answer>) -> AGitHub {
    let received = Arc::new(Mutex::new(Vec::new()));
    let desk = TheDesk {
        answers: Arc::new(Mutex::new(answers)),
        received: received.clone(),
    };
    let app = Router::new()
        .route("/login/device/code", post(answer))
        .route("/login/oauth/access_token", post(answer))
        .route("/user", get(answer))
        .with_state(desk);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback has a free port");
    let address = listener.local_addr().expect("the listener has an address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("the desk serves");
    });
    AGitHub { address, received }
}

async fn answer(
    State(desk): State<TheDesk>,
    uri: Uri,
    headers: HeaderMap,
    body: String,
) -> axum::response::Response {
    desk.received.lock().unwrap().push(AReceivedRequest {
        path_and_query: uri
            .path_and_query()
            .expect("a request GitHub receives names its path")
            .as_str()
            .to_string(),
        headers: headers
            .iter()
            .map(|(name, value)| format!("{name}: {}", String::from_utf8_lossy(value.as_bytes())))
            .collect(),
        body,
    });
    let next = {
        let mut answers = desk.answers.lock().unwrap();
        (!answers.is_empty()).then(|| answers.remove(0))
    };
    match next {
        Some(Answer::DeviceCode {
            user_code,
            interval,
        }) => a_device_code_answer(&user_code, 900, interval),
        Some(Answer::DeviceCodeExpiringIn {
            expires_in,
            interval,
        }) => a_device_code_answer("WDJB-MJHT", expires_in, interval),
        Some(Answer::PollError { error, interval }) => Json(serde_json::json!({
            "error": error,
            "interval": interval,
        }))
        .into_response(),
        Some(Answer::AccessToken(token)) => {
            Json(serde_json::json!({ "access_token": token })).into_response()
        }
        Some(Answer::User { login }) => Json(serde_json::json!({
            "id": 7,
            "login": login,
            "avatar_url": "https://example.com/a.png",
            "name": "The Operator",
        }))
        .into_response(),
        Some(Answer::Status(code)) => (
            axum::http::StatusCode::from_u16(code).expect("a real status"),
            "",
        )
            .into_response(),
        Some(Answer::Garbage) => "this is not json".into_response(),
        None => Json(serde_json::json!({})).into_response(),
    }
}

fn a_device_code_answer(
    user_code: &str,
    expires_in: u64,
    interval: u64,
) -> axum::response::Response {
    Json(serde_json::json!({
        "device_code": "the-device-code",
        "user_code": user_code,
        "verification_uri": "https://github.com/login/device",
        "expires_in": expires_in,
        "interval": interval,
    }))
    .into_response()
}
