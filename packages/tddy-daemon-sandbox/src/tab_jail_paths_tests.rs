//! `TabJailPaths`: each jailed tab's files are its own — under its own directory of the session's
//! sandbox, with a tool-IPC socket that is neither the main's nor another tab's, and short enough
//! for `SUN_LEN`.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_rpc::Code;

use super::{tab_ipc_socket_path, TabJailPaths};

const SESSION_ID: &str = "019b0000-0000-7000-8000-000000005e55";
const TAB: &str = "019b0000-0000-7000-8000-0000000000a1";
const OTHER_TAB: &str = "019b0000-0000-7000-8000-0000000000b2";

/// macOS caps `sockaddr_un.sun_path` at 104 bytes, its terminating NUL included (Linux at 108).
const SUN_PATH_BYTES: usize = 104;

fn a_session_dir() -> PathBuf {
    PathBuf::from("/var/tddy/sessions/").join(SESSION_ID)
}

/// Every file a tab's jail is started with.
fn files_of(paths: &TabJailPaths) -> Vec<PathBuf> {
    vec![
        paths.root.clone(),
        paths.ipc_socket.clone(),
        paths.ready_marker.clone(),
        paths.exec_plan.clone(),
        paths.profile.clone(),
        paths.egress_dir.clone(),
        paths.scratch_dir.clone(),
    ]
}

#[test]
fn a_tabs_jail_files_live_in_a_directory_of_its_own_under_the_session_sandbox() {
    // Given
    let session_dir = a_session_dir();

    // When
    let paths = TabJailPaths::for_tab(&session_dir, SESSION_ID, TAB).expect("a tab's paths");

    // Then
    let root = session_dir.join("sandbox").join("tabs").join(TAB);
    assert_eq!(
        paths,
        TabJailPaths {
            root: root.clone(),
            ipc_socket: tab_ipc_socket_path(SESSION_ID, TAB),
            ready_marker: root.join("sandbox.ready"),
            exec_plan: root.join("exec-plan.json"),
            profile: root.join("sandbox.sb"),
            egress_dir: root.join("egress"),
            scratch_dir: root.join(".work"),
        }
    );
}

#[test]
fn two_tabs_of_one_session_share_no_jail_file() {
    // Given
    let session_dir = a_session_dir();

    // When
    let one = TabJailPaths::for_tab(&session_dir, SESSION_ID, TAB).expect("a tab's paths");
    let other = TabJailPaths::for_tab(&session_dir, SESSION_ID, OTHER_TAB).expect("a tab's paths");

    // Then
    let others = files_of(&other);
    let shared: Vec<PathBuf> = files_of(&one)
        .into_iter()
        .filter(|file| others.contains(file))
        .collect();
    assert_eq!(shared, Vec::<PathBuf>::new());
}

#[test]
fn a_tabs_ipc_socket_is_not_its_sessions_main_socket() {
    // When
    let tab_socket = tab_ipc_socket_path(SESSION_ID, TAB);

    // Then
    assert_ne!(
        tab_socket,
        tddy_sandbox::SandboxSpec::short_ipc_socket_path(SESSION_ID)
    );
}

#[test]
fn the_ipc_sockets_of_two_tabs_of_one_session_differ() {
    // When
    let one = tab_ipc_socket_path(SESSION_ID, TAB);
    let other = tab_ipc_socket_path(SESSION_ID, OTHER_TAB);

    // Then
    assert_ne!(one, other);
}

#[test]
fn a_tabs_ipc_socket_lives_beside_its_sessions_main_socket_out_of_the_tree() {
    // When
    let tab_socket = tab_ipc_socket_path(SESSION_ID, TAB);

    // Then the socket is in the same short temp directory the main's is, not under the session
    assert_eq!(
        tab_socket.parent(),
        tddy_sandbox::SandboxSpec::short_ipc_socket_path(SESSION_ID).parent()
    );
}

#[test]
fn the_ipc_socket_of_a_tab_with_long_ids_fits_the_af_unix_limit() {
    // Given a session id far longer than a UUID, and a UUID terminal id
    let long_session_id = format!("workspace-{}", "a".repeat(200));

    // When
    let socket = tab_ipc_socket_path(&long_session_id, TAB);

    // Then — a bound, not an exact length: the temp directory the socket lives in is the host's
    assert!(
        socket.as_os_str().len() < SUN_PATH_BYTES,
        "{} is {} bytes",
        socket.display(),
        socket.as_os_str().len()
    );
}

#[test]
fn a_terminal_id_that_is_not_one_path_segment_is_refused() {
    // When
    let refused = TabJailPaths::for_tab(Path::new("/var/tddy/s"), SESSION_ID, "../../escape")
        .expect_err("a terminal id naming a path outside the session");

    // Then
    assert_eq!(refused.code(), Code::InvalidArgument);
}
