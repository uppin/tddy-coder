//! What `git` inside a session commits as — and what it deliberately still does not.

use tddy_accounts::{ActingIdentity, GitIdentity};
use tddy_credentials::AccountId;
use tddy_daemon_livekit::session_git::session_git_environment;

fn grace_acting() -> ActingIdentity {
    ActingIdentity {
        account: AccountId::new("acct-grace"),
        token: "ghp_grace_token".to_string(),
        git: GitIdentity {
            name: "grace".to_string(),
            email: "202+grace@users.noreply.github.com".to_string(),
        },
    }
}

fn value_of<'a>(env: &'a [(String, String)], name: &str) -> &'a str {
    env.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the session environment sets no {name}: {env:?}"))
}

#[test]
fn a_commit_made_in_a_session_is_authored_by_the_account_the_project_assigns() {
    // Given a session acting as the account its project assigned
    let acting = grace_acting();

    // When the session's git environment is built
    let env = session_git_environment(&acting);

    // Then a commit made in it is authored by her
    assert_eq!(
        (
            value_of(&env, "GIT_AUTHOR_NAME"),
            value_of(&env, "GIT_AUTHOR_EMAIL")
        ),
        ("grace", "202+grace@users.noreply.github.com")
    );
}

#[test]
fn a_commit_made_in_a_session_is_committed_by_the_same_account_that_authored_it() {
    // Given a session acting as the account its project assigned
    let acting = grace_acting();

    // When the session's git environment is built
    let env = session_git_environment(&acting);

    // Then the committer is that account too — a session's commit has one maker
    assert_eq!(
        (
            value_of(&env, "GIT_COMMITTER_NAME"),
            value_of(&env, "GIT_COMMITTER_EMAIL")
        ),
        ("grace", "202+grace@users.noreply.github.com")
    );
}

#[test]
fn the_session_s_git_environment_carries_the_identity_and_nothing_else() {
    // Given a session acting as an account whose token is right there in the same value
    let acting = grace_acting();

    // When the session's git environment is built
    let env = session_git_environment(&acting);

    // Then it is the four identity variables, and the credential is not among them
    let mut names: Vec<&str> = env.iter().map(|(key, _)| key.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_NAME",
        ]
    );
}

/// A tripwire, green from the moment it is written: the snapshot identity is what this node
/// promised **not** to change, and the promise is worth a test that fails if somebody later
/// attributes a machine-made measurement to a person's GitHub account.
#[test]
fn a_work_in_progress_snapshot_is_still_signed_by_the_daemon_itself() {
    // Given the module that publishes work-in-progress refs
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/session_room.rs"),
    )
    .expect("session_room.rs");

    // Then it signs them as the daemon
    assert!(
        source.contains(r#"WIP_COMMIT_IDENTITY_NAME: &str = "tddy-daemon""#),
        "the snapshot identity is no longer the daemon's own"
    );

    // And it does not reach for a person's account to do it
    assert!(
        !source.contains("session_git_environment"),
        "the snapshot commits now borrow a session's assigned identity"
    );
}
