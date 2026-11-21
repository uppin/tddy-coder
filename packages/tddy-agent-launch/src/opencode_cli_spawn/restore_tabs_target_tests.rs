//! Unit tests: the restore rule ([`restore_plan`]) for a tab recorded on its session's target —
//! `target` → `shell` reopens on the target; anything else on a target but an agent is dropped naming
//! why (`#session-liveness` 6/8; an agent on a target is `target-agent-tab`'s,
//! `restore_tabs_target_agent_tests.rs`).

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{LaunchNode, TabEntry};

use super::{restore_plan, LiveTabs, TabRestore};

const TAB: &str = "019b-target-tab";

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
fn a_shell_recorded_on_a_target_is_planned_as_a_shell_on_the_target() {
    // Given
    let tab = a_tab(vec![LaunchNode::target(), LaunchNode::shell()]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(plan, TabRestore::TargetShell);
}

#[test]
fn a_target_recording_nothing_on_it_is_dropped() {
    // Given
    let tab = a_tab(vec![LaunchNode::target()]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("target"), "{reason}");
}

#[test]
fn a_target_wrapping_a_sandbox_is_dropped_naming_both() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::sandbox(),
        LaunchNode::shell(),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then the reason names both the target and what it may not wrap
    assert!(
        reason.contains("target") && reason.contains("sandbox"),
        "{reason}"
    );
}
