//! [`super::read_tab_roster`] and [`super::write_tab_roster`]: the roster file round-trips, keeps
//! what it does not know, and reads as empty before anything wrote it.

use std::path::Path;

use pretty_assertions::assert_eq;

use super::*;

/// The session's roster file, parsed as plain YAML: what a rewrite must leave untouched.
fn the_roster_file_as_yaml(session_dir: &Path) -> serde_yaml::Value {
    let text = std::fs::read_to_string(session_dir.join(TAB_ROSTER_FILENAME))
        .expect("the roster file exists");
    serde_yaml::from_str(&text).expect("the roster file is YAML")
}

/// A session directory whose roster file holds `yaml` as written by some other daemon.
fn a_session_dir_whose_roster_reads(yaml: &str) -> tempfile::TempDir {
    let session_dir = tempfile::tempdir().unwrap();
    std::fs::write(session_dir.path().join(TAB_ROSTER_FILENAME), yaml).unwrap();
    session_dir
}

/// Read the roster and write it straight back, as every recording daemon does.
fn rewrite_the_roster(session_dir: &Path) {
    let roster = read_tab_roster(session_dir).expect("the roster reads");
    write_tab_roster(session_dir, &roster).expect("the roster writes");
}

fn a_roster_of_a_tmux_main_and_a_shell() -> TabRoster {
    TabRoster {
        tabs: vec![
            TabEntry::new(
                "main",
                vec![
                    LaunchNode::tmux("tddy-session-1", "dev"),
                    LaunchNode::agent("claude-cli", "5f3c-conversation"),
                ],
            ),
            TabEntry::new("019a-shell", vec![LaunchNode::shell()]),
        ],
        ..TabRoster::default()
    }
}

#[test]
fn a_written_roster_reads_back_as_the_same_roster() {
    // Given a session directory and a roster of a tmux-run main and a shell
    let session_dir = tempfile::tempdir().unwrap();
    let roster = a_roster_of_a_tmux_main_and_a_shell();

    // When it is written and read back
    write_tab_roster(session_dir.path(), &roster).expect("the roster writes");
    let read = read_tab_roster(session_dir.path()).expect("the roster reads");

    // Then it is the roster that was written
    assert_eq!(read, roster);
}

#[test]
fn a_roster_is_written_to_the_tabs_file_beside_the_session_record() {
    // Given a session directory and a roster
    let session_dir = tempfile::tempdir().unwrap();

    // When it is written
    write_tab_roster(session_dir.path(), &a_roster_of_a_tmux_main_and_a_shell())
        .expect("the roster writes");

    // Then the tabs file holds each tab's launch chain, outermost first
    assert_eq!(
        the_roster_file_as_yaml(session_dir.path()),
        serde_yaml::from_str::<serde_yaml::Value>(
            "tabs:\n\
             - terminal_id: main\n  \
               launch:\n  \
               - kind: tmux\n    properties: {session: tddy-session-1, os_user: dev}\n  \
               - kind: agent\n    properties: {harness: claude-cli, conversation_id: 5f3c-conversation}\n\
             - terminal_id: 019a-shell\n  \
               launch:\n  \
               - kind: shell\n    properties: {}\n"
        )
        .unwrap()
    );
}

#[test]
fn an_unknown_wrapper_kind_and_property_key_survive_a_rewrite() {
    // Given a roster a newer daemon wrote: an `ssh` wrapper, and a property this daemon never reads
    let session_dir = a_session_dir_whose_roster_reads(
        "tabs:\n\
         - terminal_id: 019a-remote\n  \
           launch:\n  \
           - kind: ssh\n    properties: {host: build-box, port: '2222'}\n  \
           - kind: tmux\n    properties: {session: tddy-s-019a-remote, os_user: dev, socket: alt}\n  \
           - kind: agent\n    properties: {harness: claude-cli, conversation_id: c-1}\n",
    );
    let before = the_roster_file_as_yaml(session_dir.path());

    // When this daemon rewrites the roster
    rewrite_the_roster(session_dir.path());

    // Then the file says exactly what it said before
    assert_eq!(the_roster_file_as_yaml(session_dir.path()), before);
}

#[test]
fn unknown_top_level_entry_and_node_keys_survive_a_rewrite() {
    // Given a roster carrying keys this daemon's model has no field for, at every level
    let session_dir = a_session_dir_whose_roster_reads(
        "version: 2\n\
         tabs:\n\
         - terminal_id: main\n  \
           restored_from: 019a-old\n  \
           launch:\n  \
           - kind: agent\n    properties: {harness: muse-cli, conversation_id: c-2}\n    \
             resumable: false\n",
    );
    let before = the_roster_file_as_yaml(session_dir.path());

    // When this daemon rewrites the roster
    rewrite_the_roster(session_dir.path());

    // Then every unknown key is still there, unchanged
    assert_eq!(the_roster_file_as_yaml(session_dir.path()), before);
}

#[test]
fn a_session_without_a_roster_file_reads_as_an_empty_roster() {
    // Given a session directory nothing has recorded a tab in
    let session_dir = tempfile::tempdir().unwrap();

    // When its roster is read
    let roster = read_tab_roster(session_dir.path()).expect("an absent roster reads");

    // Then it holds no tabs
    assert_eq!(roster, TabRoster::default());
}

#[test]
fn a_launch_node_answers_its_known_properties() {
    // Given a tmux node
    let node = LaunchNode::tmux("tddy-session-1", "dev");

    // When / Then its properties are read by key, and an absent one is none
    assert_eq!(node.kind, TMUX_KIND);
    assert_eq!(node.property(SESSION_PROPERTY), Some("tddy-session-1"));
    assert_eq!(node.property(OS_USER_PROPERTY), Some("dev"));
    assert_eq!(node.property(HARNESS_PROPERTY), None);
}
