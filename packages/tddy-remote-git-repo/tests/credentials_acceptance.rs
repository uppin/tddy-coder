//! Acceptance: credential resolution — flags with per-parameter environment fallback.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § Credentials, § Client AC5.
//!
//! A `GIT_SSH_COMMAND` is configured once and reused by every git command, so a missing credential
//! must name both the flag and the environment variable that would have supplied it. Guessing, or
//! connecting part-way and failing later, would surface as an unexplained "could not read from
//! remote repository".
//!
//! The client holds **one** credential and **one** address: a daemon token and the daemon's URL.
//! It never holds `LIVEKIT_API_SECRET` — that secret also signs every daemon session token, so a
//! client holding it could mint an access token for any GitHub user on the fleet. The LiveKit room
//! JWT is minted by the daemon instead (`auth.LiveKitTokenService/MintLiveKitToken`).

use std::collections::HashMap;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_remote_git_repo::credentials::{
    resolve_credentials, CredentialArgs, CredentialError, DaemonToken, DEFAULT_CONNECT_TIMEOUT,
};

fn no_env() -> HashMap<String, String> {
    HashMap::new()
}

fn an_environment(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Flags carrying everything required, and nothing optional.
fn complete_args() -> CredentialArgs {
    CredentialArgs {
        daemon_url: Some("http://udoo-1.example:8899".into()),
        session_token: Some("access-token".into()),
        ..CredentialArgs::default()
    }
}

#[test]
fn resolves_every_required_parameter_from_its_flag() {
    // Given every required credential given as a flag
    let args = complete_args();

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("complete flags must resolve");

    // Then
    assert_eq!(credentials.daemon_url, "http://udoo-1.example:8899");
    assert_eq!(
        credentials.token,
        DaemonToken::Access("access-token".into())
    );
}

#[test]
fn falls_back_to_the_environment_for_a_parameter_no_flag_supplied() {
    // Given no flags at all, and a fully populated environment
    let args = CredentialArgs::default();
    let env = an_environment(&[
        ("TDDY_DAEMON_URL", "http://from-env:8899"),
        ("TDDY_SESSION_TOKEN", "env-access-token"),
    ]);

    // When
    let credentials =
        resolve_credentials(&args, &env).expect("a complete environment must resolve");

    // Then
    assert_eq!(credentials.daemon_url, "http://from-env:8899");
    assert_eq!(
        credentials.token,
        DaemonToken::Access("env-access-token".into())
    );
}

#[test]
fn prefers_the_flag_over_the_environment_for_the_same_parameter() {
    // Given a flag and an environment variable that disagree
    let args = complete_args();
    let env = an_environment(&[("TDDY_DAEMON_URL", "http://from-env:8899")]);

    // When
    let credentials = resolve_credentials(&args, &env).expect("must resolve");

    // Then the explicit flag wins
    assert_eq!(credentials.daemon_url, "http://udoo-1.example:8899");
}

#[test]
fn ignores_a_livekit_environment_because_it_no_longer_configures_this_client() {
    // Given the LiveKit environment that used to configure this client, and no daemon address
    let env = an_environment(&[
        ("LIVEKIT_URL", "ws://livekit.example:7880"),
        ("LIVEKIT_API_KEY", "devkey"),
        ("LIVEKIT_API_SECRET", "the-fleet-signing-secret"),
        ("TDDY_LIVEKIT_ROOM", "tddy-lobby"),
        ("TDDY_SESSION_TOKEN", "access-token"),
    ]);

    // When
    let error = resolve_credentials(&CredentialArgs::default(), &env)
        .expect_err("a LiveKit environment must not stand in for the daemon's address");

    // Then none of those values is read: the room JWT is minted by the daemon, and the API secret
    // — which also signs every session token on the fleet — has no business on a git client
    assert_eq!(
        error,
        CredentialError::Missing {
            flag: "--daemon-url",
            env_var: "TDDY_DAEMON_URL",
        }
    );
}

#[test]
fn defaults_the_connect_timeout_when_none_is_configured() {
    // Given no timeout given anywhere
    let credentials = resolve_credentials(&complete_args(), &no_env()).expect("must resolve");

    // Then
    assert_eq!(credentials.connect_timeout, DEFAULT_CONNECT_TIMEOUT);
}

#[test]
fn honours_a_configured_connect_timeout() {
    // Given a five-second timeout
    let args = CredentialArgs {
        connect_timeout_secs: Some(5),
        ..complete_args()
    };

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then
    assert_eq!(credentials.connect_timeout, Duration::from_secs(5));
}

#[test]
fn accepts_a_refresh_token_as_the_credential_because_an_access_token_expires_in_five_minutes() {
    // Given only a refresh token — the credential a developer can configure once
    let args = CredentialArgs {
        session_token: None,
        refresh_token: Some("refresh-token".into()),
        ..complete_args()
    };

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("a refresh token must resolve");

    // Then
    assert_eq!(
        credentials.token,
        DaemonToken::Refresh("refresh-token".into())
    );
}

#[test]
fn prefers_an_access_token_over_a_refresh_token_because_it_needs_no_exchange() {
    // Given both tokens
    let args = CredentialArgs {
        refresh_token: Some("refresh-token".into()),
        ..complete_args()
    };

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then the access token is used directly, saving a RefreshSession round trip
    assert_eq!(
        credentials.token,
        DaemonToken::Access("access-token".into())
    );
}

#[test]
fn reports_a_missing_daemon_url_by_naming_both_the_flag_and_the_environment_variable() {
    // Given every credential but the daemon's address
    let args = CredentialArgs {
        daemon_url: None,
        ..complete_args()
    };

    // When
    let error = resolve_credentials(&args, &no_env()).expect_err("must refuse to connect");

    // Then
    assert_eq!(
        error,
        CredentialError::Missing {
            flag: "--daemon-url",
            env_var: "TDDY_DAEMON_URL",
        }
    );
    let message = error.to_string();
    assert!(
        message.contains("--daemon-url") && message.contains("TDDY_DAEMON_URL"),
        "message must name both ways to supply it, got: {message}"
    );
}

#[test]
fn reports_that_neither_token_was_supplied_when_both_are_absent() {
    // Given a daemon address but no daemon credential of either kind
    let args = CredentialArgs {
        session_token: None,
        refresh_token: None,
        ..complete_args()
    };

    // When
    let error = resolve_credentials(&args, &no_env()).expect_err("must refuse to connect");

    // Then
    assert_eq!(error, CredentialError::MissingToken);
    let message = error.to_string();
    assert!(
        message.contains("--session-token") && message.contains("--refresh-token"),
        "message must name both credentials that would satisfy it, got: {message}"
    );
}

#[test]
fn reports_an_unparsable_connect_timeout_rather_than_falling_back_to_the_default() {
    // Given a timeout the environment spells wrong
    let env = an_environment(&[("TDDY_CONNECT_TIMEOUT_SECS", "thirty")]);

    // When
    let error = resolve_credentials(&complete_args(), &env).expect_err("must refuse to connect");

    // Then the value is named rather than silently replaced by the default — a client that waited
    // 30s when the operator asked for 2 is a fault they would never see
    assert_eq!(
        error,
        CredentialError::Malformed {
            flag: "--connect-timeout-secs",
            env_var: "TDDY_CONNECT_TIMEOUT_SECS",
            value: "thirty".to_string(),
        }
    );
}

#[test]
fn exits_with_sshs_transport_failure_code_when_a_credential_is_missing() {
    // Given a missing credential
    let error = resolve_credentials(
        &CredentialArgs {
            daemon_url: None,
            ..complete_args()
        },
        &no_env(),
    )
    .expect_err("must refuse to connect");

    // When / Then 255 is what git reads as "could not reach the remote"
    assert_eq!(error.exit_code(), 255);
}
