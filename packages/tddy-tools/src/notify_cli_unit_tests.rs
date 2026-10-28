//! Unit tests: how `tddy-tools notify` reads its session from the environment and builds its
//! request (`#home-inbox` 1/8). The process-level behaviour is pinned in
//! `tddy-tools-cli/tests/notify_cli_acceptance.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

use super::*;

const SESSION: &str = "01a19c2e-55d0-7a41-9b3e-4f1d2c3b4a59";

/// An environment holding exactly `vars`.
fn environment(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    move |name| vars.get(name).cloned()
}

/// A daemon-launched session's credentials, plus `daemon` naming how its daemon is reached.
fn a_session_environment<'a>(daemon: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut vars = vec![
        (notify_env::SESSION_ID, SESSION),
        (notify_env::OS_USER, "ada"),
        (notify_env::HOOK_TOKEN, "the-hook-token"),
    ];
    vars.extend_from_slice(daemon);
    vars
}

fn args(title: &str, level: NotifyLevel, sound: bool) -> NotifyArgs {
    NotifyArgs {
        title: title.to_string(),
        body: None,
        level,
        sound,
    }
}

#[test]
fn a_session_on_an_http_host_is_reached_at_its_url() {
    let session = NotifySession::from_lookup(environment(&a_session_environment(&[(
        notify_env::DAEMON_URL,
        "http://127.0.0.1:8899",
    )])))
    .expect("a complete session environment");

    assert_eq!(
        session,
        NotifySession {
            session_id: SESSION.to_string(),
            os_user: "ada".to_string(),
            hook_token: "the-hook-token".to_string(),
            transport: Transport::Http("http://127.0.0.1:8899".to_string()),
        }
    );
}

#[test]
fn the_socket_wins_over_the_url_and_an_empty_one_does_not() {
    let both = a_session_environment(&[
        (notify_env::DAEMON_SOCKET, "/tmp/agent-tool.sock"),
        (notify_env::DAEMON_URL, "http://127.0.0.1:8899"),
    ]);
    let empty_socket = a_session_environment(&[
        (notify_env::DAEMON_SOCKET, ""),
        (notify_env::DAEMON_URL, "http://127.0.0.1:8899"),
    ]);

    let transports = [both, empty_socket].map(|vars| {
        NotifySession::from_lookup(environment(&vars))
            .expect("a complete session environment")
            .transport
    });

    assert_eq!(
        transports,
        [
            Transport::Socket(PathBuf::from("/tmp/agent-tool.sock")),
            Transport::Http("http://127.0.0.1:8899".to_string()),
        ]
    );
}

#[test]
fn a_missing_or_empty_credential_is_refused_naming_it() {
    let without_token = environment(&[
        (notify_env::SESSION_ID, SESSION),
        (notify_env::OS_USER, "ada"),
        (notify_env::HOOK_TOKEN, ""),
        (notify_env::DAEMON_URL, "http://127.0.0.1:8899"),
    ]);

    let refusal = NotifySession::from_lookup(without_token).expect_err("an empty hook token");

    assert!(
        refusal.to_string().contains(notify_env::HOOK_TOKEN),
        "the refusal names the variable: {refusal}"
    );
}

#[test]
fn a_session_with_no_daemon_named_is_refused_naming_both_variables() {
    let refusal = NotifySession::from_lookup(environment(&a_session_environment(&[])))
        .expect_err("no daemon variable");

    let message = refusal.to_string();
    assert!(
        message.contains(notify_env::DAEMON_SOCKET) && message.contains(notify_env::DAEMON_URL),
        "the refusal names both ways to reach the daemon: {message}"
    );
}

#[test]
fn the_request_carries_the_sessions_credentials_and_the_agents_words() {
    let session = NotifySession::from_lookup(environment(&a_session_environment(&[(
        notify_env::DAEMON_URL,
        "http://127.0.0.1:8899",
    )])))
    .expect("a complete session environment");

    let request = session.request(
        &args("Blocked on review", NotifyLevel::Warning, false),
        "needs **you**".to_string(),
    );

    assert_eq!(
        request,
        ReportOperatorNotificationRequest {
            session_id: SESSION.to_string(),
            hook_token: "the-hook-token".to_string(),
            os_user: "ada".to_string(),
            title: "Blocked on review".to_string(),
            body_markdown: "needs **you**".to_string(),
            level: NotificationLevel::Warning as i32,
            sound: false,
        }
    );
}

#[test]
fn every_cli_level_maps_to_its_wire_level() {
    let levels = [
        NotifyLevel::Info,
        NotifyLevel::Success,
        NotifyLevel::Warning,
        NotifyLevel::Error,
    ]
    .map(NotificationLevel::from);

    assert_eq!(
        levels,
        [
            NotificationLevel::Info,
            NotificationLevel::Success,
            NotificationLevel::Warning,
            NotificationLevel::Error,
        ]
    );
}

#[tokio::test]
async fn a_body_given_inline_is_sent_as_is_and_no_body_is_empty() {
    let bodies = (
        body_markdown(Some("4 done, **1 blocked**"))
            .await
            .expect("an inline body"),
        body_markdown(None).await.expect("no body"),
    );

    assert_eq!(bodies, ("4 done, **1 blocked**".to_string(), String::new()));
}
