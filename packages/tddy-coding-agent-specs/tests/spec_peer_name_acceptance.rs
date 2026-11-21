//! An agent with native peer messaging declares the flag a session's peer name is passed with
//! (`coding_agent.peer_name`, `#orchestra` 2/9). Claude Code's is `--name`; an agent without it has
//! no native channel, and its children report through `tddy-tools`.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Session messages.

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{embedded_spec, PeerNameSlot};

#[test]
fn claude_declares_its_peer_name_flag() {
    // When
    let claude = embedded_spec("claude").expect("claude is built in");

    // Then
    assert_eq!(
        claude.peer_name(),
        Some(&PeerNameSlot {
            flag: "--name".to_string()
        })
    );
}

#[test]
fn an_agent_without_native_peer_messaging_declares_no_peer_name() {
    // When
    let codex = embedded_spec("codex").expect("codex is built in");

    // Then
    assert_eq!(codex.peer_name(), None);
}
