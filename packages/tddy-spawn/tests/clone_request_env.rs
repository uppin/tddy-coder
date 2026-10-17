//! The forked-worker clone carries environment.
//!
//! Until now a worker clone could not: `CloneRequest` was `{os_user, git_url, destination}`, so the
//! SSH settings a clone needs could only reach the in-process fallback. The request travels as JSON
//! between the daemon and a worker that may be a different build, so the field has to be optional on
//! the wire.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC13)

use std::collections::BTreeMap;

use tddy_spawn::spawn_worker::CloneRequest;

#[test]
fn a_request_carries_the_environment_it_was_given() {
    let request = CloneRequest {
        os_user: "mantas".into(),
        git_url: "git@github.com:uppin/tddy-coder.git".into(),
        destination: "/home/mantas/repos/tddy-coder".into(),
        env: BTreeMap::from([("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string())]),
    };

    let round_trip: CloneRequest =
        serde_json::from_str(&serde_json::to_string(&request).expect("serialises"))
            .expect("parses");

    assert_eq!(round_trip.env["SSH_ASKPASS_REQUIRE"], "force");
}

#[test]
fn a_request_from_a_daemon_that_predates_the_field_still_parses_with_no_environment() {
    let from_an_older_daemon = r#"{"os_user":"mantas","git_url":"u","destination":"/d"}"#;

    let request: CloneRequest = serde_json::from_str(from_an_older_daemon).expect("parses");

    assert!(request.env.is_empty());
}
