//! The user mapping lives in `users.yaml`, and the supervisor serves one daemon socket per mapped OS
//! user from it (`#agent-tools` 31/32): handed to the daemon at exec with the file's path, and
//! rebuilt — the daemon restarted — when the file is saved.
//!
//! Every test here needs a service the supervisor actually started, so the whole file is
//! Linux-only — see the note in `support`'s header. The decisions underneath (the mapping, the
//! socket set and its rebuild, the handover's names, the watch) are asserted on every host by the
//! unit tests in `users_file`, `user_sockets`, `handover` and `users_watch`.
//!
//! Feature docs: docs/ft/supervisor/tddy-supervisor.md
//! Changeset: docs/dev/1-WIP/2026-11-07-user-mapping-in-supervisor.md
#![cfg(target_os = "linux")]

mod support;

use support::{a_service, a_supervisor, current_username};

#[tokio::test]
async fn the_daemon_is_handed_the_users_file_and_one_socket_per_mapped_user_at_exec() {
    // Given users.yaml maps this account, and a daemon that serves users
    let supervisor = a_supervisor()
        .mapping_the_current_user_as(&["work"])
        .managing(
            a_service("tddy-daemon")
                .with_a_listening_socket()
                .serving_users(),
        )
        .start()
        .await;

    // When the daemon reports what it was handed
    let report = supervisor.await_handoff_report("tddy-daemon").await;

    // Then its own listener, then one generic socket for the user, and the mapping's path
    assert_eq!(
        (
            report.get("listen_fdnames").cloned(),
            report.get("users_file").cloned(),
        ),
        (
            Some(format!("connection:user.{}", current_username())),
            Some(supervisor.users_file_path().display().to_string()),
        ),
        "{report:?}"
    );
    assert!(
        supervisor.derived_user_socket_path().exists(),
        "the user's socket is at its derived path"
    );
}

#[tokio::test]
async fn a_changed_users_yaml_rebuilds_the_listeners_keeping_unchanged_descriptors_and_restarts_the_daemon(
) {
    // Given a running daemon handed this account's socket
    let supervisor = a_supervisor()
        .mapping_the_current_user_as(&["work"])
        .managing(
            a_service("tddy-daemon")
                .with_a_listening_socket()
                .serving_users(),
        )
        .start()
        .await;
    let before = supervisor.await_handoff_report("tddy-daemon").await;
    supervisor.clear_handoff_report("tddy-daemon");

    // When users.yaml is saved with a second login for the same account
    supervisor.remap_the_current_user_as(&["work", "home"]);

    // Then the daemon is restarted, and handed the very same listener for the unchanged user
    let after = supervisor.await_handoff_report("tddy-daemon").await;
    assert_ne!(
        after.get("own_pid"),
        before.get("own_pid"),
        "the daemon must restart"
    );
    assert_eq!(
        after.get("fd4_inode"),
        before.get("fd4_inode"),
        "an unchanged user keeps their descriptor: {before:?} → {after:?}"
    );
}
