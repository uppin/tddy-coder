//! Unit tests: the restore rule ([`restore_plan`]) for a tab recorded in a jail of its own —
//! `sandbox` → `shell` reopens in a new jail; anything else in a jail is dropped naming why
//! (`#session-liveness` 5/8; an agent in a jail is `sandbox-agent-tab`'s).

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{LaunchNode, TabEntry};

use super::{restore_plan, LiveTabs, TabRestore};

const TAB: &str = "019b-jailed-tab";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-000000000002";

fn a_tab(launch: Vec<LaunchNode>) -> TabEntry {
    TabEntry::new(TAB, launch)
}

/// The reason of a dropped tab, failing the test when the tab is not dropped.
fn dropped_reason(plan: TabRestore) -> String {
    let TabRestore::Drop { reason } = plan else {
        panic!("expected the tab to be dropped, got {plan:?}");
    };
    reason
}

#[test]
fn a_shell_recorded_in_a_sandbox_is_planned_as_a_shell_in_a_new_jail() {
    // Given
    let tab = a_tab(vec![LaunchNode::sandbox(), LaunchNode::shell()]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(plan, TabRestore::JailedShell);
}

#[test]
fn an_agent_recorded_in_a_sandbox_is_dropped_naming_the_agent() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::sandbox(),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("agent"), "{reason}");
}

#[test]
fn a_sandbox_recording_nothing_inside_it_is_dropped() {
    // Given
    let tab = a_tab(vec![LaunchNode::sandbox()]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("sandbox"), "{reason}");
}

#[test]
fn a_sandbox_wrapping_a_tmux_session_is_dropped() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::sandbox(),
        LaunchNode::tmux("tddy-019a-session-019b-tab", "dev"),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then the reason names both the jail and what it may not wrap
    assert!(
        reason.contains("sandbox") && reason.contains("tmux"),
        "{reason}"
    );
}
