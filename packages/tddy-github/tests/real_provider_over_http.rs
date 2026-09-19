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
    // Given a provider that holds a client secret, and a GitHub recording what it receives
    let github = a_github_answering(vec![
        Answer::DeviceCode {
            user_code: "WDJB-MJHT".to_string(),
            interval: 5,
        },
        Answer::PollError {
            error: "authorization_pending".to_string(),
            interval: None,
        },
    ])
    .await;
    let provider = the_provider(&github);

    // When a whole device login is started and polled
    let started = provider
        .start_device_login()
        .await
        .expect("GitHub issued a device code");
    let _ = provider.poll_device_login(&started.device_code).await;

    // Then the secret never left this process — the flow exists precisely so it need not
    assert_eq!(
        github
            .received
            .lock()
            .unwrap()
            .iter()
            .filter(|body| body.contains(THE_SECRET))
            .count(),
        0,
        "the device flow authenticates with a public client id alone; bodies were {:?}",
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
    let (_, state) = provider.authorize_url();
    let exchanged = provider.exchange_code("the-code", &state).await;

    // Then both halves come back
    assert_eq!(
        exchanged.map(|(token, user)| (token, user.login)),
        Ok(("gho_a-granted-token".to_string(), "operator".to_string()))
    );
}

/// Run one exchange against a GitHub, through a state that provider actually issued.
async fn exchange_against(github: &AGitHub) -> Result<(), String> {
    let provider = the_provider(github);
    let (_, state) = provider.authorize_url();
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

/// One canned reply, in the order the provider will ask for them.
#[derive(Clone)]
enum Answer {
    DeviceCode {
        user_code: String,
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
/// keeping every request body it was sent.
struct AGitHub {
    address: SocketAddr,
    received: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct TheDesk {
    answers: Arc<Mutex<Vec<Answer>>>,
    received: Arc<Mutex<Vec<String>>>,
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

async fn answer(State(desk): State<TheDesk>, body: String) -> axum::response::Response {
    desk.received.lock().unwrap().push(body);
    let next = {
        let mut answers = desk.answers.lock().unwrap();
        (!answers.is_empty()).then(|| answers.remove(0))
    };
    match next {
        Some(Answer::DeviceCode {
            user_code,
            interval,
        }) => Json(serde_json::json!({
            "device_code": "the-device-code",
            "user_code": user_code,
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": interval,
        }))
        .into_response(),
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

use axum::response::IntoResponse;
