//! Whether `subagent_diff` / `subagent_pull` / `subagent_end` have a conversation worktree to read
//! and take.
//!
//! Feature doc: docs/ft/coder/managed-codebase-subagents.md § A daemon-run conversation of a `clone`-placed agent

use tddy_tools::worktree_route::{has_its_own_worktree, ConversationHost};

#[test]
fn an_in_process_conversation_under_managed_access_has_its_own_worktree() {
    assert!(has_its_own_worktree(ConversationHost::InProcess {
        managed_access: true
    }));
}

#[test]
fn an_in_process_conversation_reading_the_host_directly_has_none() {
    assert!(!has_its_own_worktree(ConversationHost::InProcess {
        managed_access: false
    }));
}

#[test]
fn a_daemon_run_conversation_of_a_clone_placed_agent_has_its_own_worktree() {
    assert!(has_its_own_worktree(ConversationHost::Daemon {
        clone: true
    }));
}

#[test]
fn a_daemon_run_conversation_of_a_shared_agent_has_none() {
    assert!(!has_its_own_worktree(ConversationHost::Daemon {
        clone: false
    }));
}
