//! Credential resolution — AC21-AC24 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Pure: the environment is injected as a map, so nothing here reads or writes the process
//! environment and every case runs in parallel with every other.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_session_sync::{
    layered_environment, parse_env_file, resolve_credentials, CredentialError, DaemonToken,
    LiveKitCredentials, SyncArgs,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// Every parameter given by flag, so a test overrides only the one it is about.
fn complete_args() -> SyncArgs {
    SyncArgs {
        session_id: Some("1780828020298-abc".to_string()),
        dest: Some(PathBuf::from("/tmp/mirror")),
        livekit_url: Some("ws://127.0.0.1:7880".to_string()),
        livekit_api_key: Some("devkey".to_string()),
        livekit_api_secret: Some("secret".to_string()),
        daemon_url: Some("http://udoo-1.example:8899".to_string()),
        session_token: Some("access-token".to_string()),
        refresh_token: None,
        connect_timeout_secs: None,
    }
}

fn no_env() -> HashMap<String, String> {
    HashMap::new()
}

fn an_environment(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The error a resolution produced, for a test that is about the refusal rather than the result.
fn rejected(args: &SyncArgs, env: &HashMap<String, String>) -> CredentialError {
    match resolve_credentials(args, env) {
        Err(e) => e,
        Ok(credentials) => panic!("expected a refusal but resolved {credentials:?}"),
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[test]
fn resolves_every_parameter_from_flags_alone() {
    // Given every parameter on the command line and an empty environment
    let args = complete_args();

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then
    assert_eq!(credentials.session_id, "1780828020298-abc");
    assert_eq!(credentials.dest, PathBuf::from("/tmp/mirror"));
    assert_eq!(
        credentials.livekit,
        LiveKitCredentials {
            url: "ws://127.0.0.1:7880".to_string(),
            api_key: "devkey".to_string(),
            api_secret: "secret".to_string(),
        }
    );
    assert_eq!(credentials.daemon_url, "http://udoo-1.example:8899");
    assert_eq!(
        credentials.token,
        DaemonToken::Access("access-token".to_string())
    );
}

#[test]
fn falls_back_to_the_environment_for_a_parameter_no_flag_gave() {
    // Given no --livekit-url on the command line
    let args = SyncArgs {
        livekit_url: None,
        ..complete_args()
    };
    let env = an_environment(&[("LIVEKIT_URL", "ws://from-env:7880")]);

    // When
    let credentials = resolve_credentials(&args, &env).expect("must resolve");

    // Then
    assert_eq!(credentials.livekit.url, "ws://from-env:7880");
}

#[test]
fn prefers_the_flag_over_the_environment_variable_that_backs_it() {
    // Given both a flag and its environment variable
    let args = complete_args();
    let env = an_environment(&[("LIVEKIT_URL", "ws://from-env:7880")]);

    // When
    let credentials = resolve_credentials(&args, &env).expect("must resolve");

    // Then the explicit flag wins — an environment left over from another run must never
    // silently redirect a command the user spelled out.
    assert_eq!(credentials.livekit.url, "ws://127.0.0.1:7880");
}

#[test]
fn prefers_an_access_token_over_a_refresh_token_when_both_are_given() {
    // Given both kinds of daemon credential
    let args = SyncArgs {
        session_token: Some("access-token".to_string()),
        refresh_token: Some("refresh-token".to_string()),
        ..complete_args()
    };

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then the access token is used: exchanging the refresh token would cost a round trip the
    // access token has already paid for.
    assert_eq!(
        credentials.token,
        DaemonToken::Access("access-token".to_string())
    );
}

#[test]
fn accepts_a_refresh_token_as_the_only_daemon_credential() {
    // Given only a refresh token
    let args = SyncArgs {
        session_token: None,
        refresh_token: Some("refresh-token".to_string()),
        ..complete_args()
    };

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then
    assert_eq!(
        credentials.token,
        DaemonToken::Refresh("refresh-token".to_string())
    );
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

#[rstest]
#[case::session_id("--session-id", "TDDY_SESSION_ID")]
#[case::livekit_url("--livekit-url", "LIVEKIT_URL")]
#[case::livekit_api_key("--livekit-api-key", "LIVEKIT_API_KEY")]
#[case::livekit_api_secret("--livekit-api-secret", "LIVEKIT_API_SECRET")]
#[case::daemon_url("--daemon-url", "TDDY_DAEMON_URL")]
fn names_the_environment_variable_of_a_missing_credential(
    #[case] flag: &str,
    #[case] env_var: &str,
) {
    // Given every parameter except one
    let mut args = complete_args();
    match flag {
        "--session-id" => args.session_id = None,
        "--livekit-url" => args.livekit_url = None,
        "--livekit-api-key" => args.livekit_api_key = None,
        "--livekit-api-secret" => args.livekit_api_secret = None,
        "--daemon-url" => args.daemon_url = None,
        other => panic!("unhandled flag in case table: {other}"),
    }

    // When
    let error = rejected(&args, &no_env());

    // Then both the flag and its variable are named — a user who set one is usually reaching
    // for the other, and naming only one sends them to the wrong place.
    assert_eq!(
        error,
        CredentialError::Missing {
            flag: flag.to_string(),
            env_var: env_var.to_string(),
        }
    );
}

#[test]
fn refuses_a_run_with_neither_a_session_token_nor_a_refresh_token() {
    // Given no daemon credential of either kind
    let args = SyncArgs {
        session_token: None,
        refresh_token: None,
        ..complete_args()
    };

    // When
    let error = rejected(&args, &no_env());

    // Then
    assert_eq!(error, CredentialError::MissingToken);
}

#[test]
fn refuses_a_run_with_no_destination_without_offering_an_environment_variable() {
    // Given no --dest, and an environment that sets everything it possibly could
    let args = SyncArgs {
        dest: None,
        ..complete_args()
    };
    let env = an_environment(&[
        ("TDDY_SESSION_SYNC_DEST", "/tmp/from-env"),
        ("TDDY_DEST", "/tmp/from-env"),
    ]);

    // When
    let error = rejected(&args, &env);

    // Then it is refused, and the message does not send the user looking for a variable that
    // does not exist. The directory whose contents get discarded on every sync is named on the
    // command line or not at all — never inherited from a shell set up for another run.
    assert_eq!(error, CredentialError::MissingDest);
    assert_eq!(
        error.to_string(),
        "missing --dest: the directory to mirror has no environment variable \
         and must be given on the command line"
    );
}

#[test]
fn refuses_a_connect_timeout_that_is_not_a_number_rather_than_defaulting_to_thirty() {
    // Given an unparsable timeout
    let args = SyncArgs {
        connect_timeout_secs: Some("soon".to_string()),
        ..complete_args()
    };

    // When
    let error = rejected(&args, &no_env());

    // Then it is refused, not replaced: a silently substituted default is how a misconfigured
    // run looks exactly like a working one.
    assert_eq!(
        error,
        CredentialError::Malformed {
            flag: "--connect-timeout-secs".to_string(),
            env_var: "TDDY_CONNECT_TIMEOUT_SECS".to_string(),
            reason: "invalid digit found in string".to_string(),
        }
    );
}

#[test]
fn defaults_the_connect_timeout_when_neither_flag_nor_environment_gives_one() {
    // Given no timeout anywhere
    let args = complete_args();

    // When
    let credentials = resolve_credentials(&args, &no_env()).expect("must resolve");

    // Then
    assert_eq!(credentials.connect_timeout, Duration::from_secs(30));
}

#[test]
fn exits_non_zero_for_every_credential_refusal() {
    // Given a refusal
    let error = CredentialError::MissingToken;

    // When / Then — a zero exit on a refused command line would let a script that never
    // connected report success.
    assert_eq!(error.exit_code(), 2);
}

// ---------------------------------------------------------------------------
// The repo-root .env reader
// ---------------------------------------------------------------------------

#[test]
fn reads_a_key_and_value_from_an_env_line() {
    // Given
    let contents = "LIVEKIT_URL=ws://127.0.0.1:7880\n";

    // When
    let pairs = parse_env_file(contents);

    // Then
    assert_eq!(
        pairs,
        vec![("LIVEKIT_URL".to_string(), "ws://127.0.0.1:7880".to_string())]
    );
}

#[test]
fn skips_comments_and_blank_lines() {
    // Given
    let contents = "# a comment\n\nLIVEKIT_API_KEY=devkey\n\n";

    // When
    let pairs = parse_env_file(contents);

    // Then
    assert_eq!(
        pairs,
        vec![("LIVEKIT_API_KEY".to_string(), "devkey".to_string())]
    );
}

#[rstest]
#[case::double_quoted("KEY=\"quoted\"", "quoted")]
#[case::single_quoted("KEY='quoted'", "quoted")]
#[case::unquoted("KEY=bare", "bare")]
fn strips_one_layer_of_surrounding_quotes(#[case] line: &str, #[case] expected: &str) {
    // Given / When
    let pairs = parse_env_file(line);

    // Then — one layer only, matching what the shell would have done for `./web-dev`.
    assert_eq!(pairs, vec![("KEY".to_string(), expected.to_string())]);
}

#[test]
fn fills_a_gap_the_process_environment_left_from_the_env_file() {
    // Given a variable set only in .env
    let process_env = an_environment(&[("LIVEKIT_API_KEY", "devkey")]);

    // When
    let env = layered_environment(process_env, Some("LIVEKIT_URL=ws://from-file:7880\n"));

    // Then
    assert_eq!(
        env.get("LIVEKIT_URL").map(String::as_str),
        Some("ws://from-file:7880")
    );
    assert_eq!(
        env.get("LIVEKIT_API_KEY").map(String::as_str),
        Some("devkey")
    );
}

#[test]
fn lets_an_already_exported_variable_win_over_the_env_file() {
    // Given the same variable in both, disagreeing
    let process_env = an_environment(&[("LIVEKIT_URL", "ws://exported:7880")]);

    // When
    let env = layered_environment(process_env, Some("LIVEKIT_URL=ws://from-file:7880\n"));

    // Then the export wins. A developer pointing one run at a different server must not have it
    // silently redirected — the run would succeed against the wrong server and look exactly like
    // the one they asked for.
    assert_eq!(
        env.get("LIVEKIT_URL").map(String::as_str),
        Some("ws://exported:7880")
    );
}

#[test]
fn treats_an_absent_env_file_as_contributing_nothing() {
    // Given no .env at all, which is the normal case for a gitignored per-developer file
    let process_env = an_environment(&[("LIVEKIT_URL", "ws://exported:7880")]);

    // When
    let env = layered_environment(process_env, None);

    // Then it is not an error, and the environment is unchanged.
    assert_eq!(env.len(), 1);
    assert_eq!(
        env.get("LIVEKIT_URL").map(String::as_str),
        Some("ws://exported:7880")
    );
}

#[test]
fn keeps_a_value_that_itself_contains_an_equals_sign() {
    // Given a base64-ish secret, which routinely ends in '='
    let contents = "LIVEKIT_API_SECRET=c2VjcmV0Cg==\n";

    // When
    let pairs = parse_env_file(contents);

    // Then only the FIRST '=' separates key from value.
    assert_eq!(
        pairs,
        vec![("LIVEKIT_API_SECRET".to_string(), "c2VjcmV0Cg==".to_string())]
    );
}
