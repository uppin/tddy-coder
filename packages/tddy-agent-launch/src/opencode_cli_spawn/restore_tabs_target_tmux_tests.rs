//! Unit tests: the restore rule ([`restore_plan`]) for the tmux tabs of a target session
//! (`#target-tmux` 2/2) — `target` → `tmux-attach` and `target` → `tmux` → `agent` are brought back
//! on the target whether or not their tmux session lives there (it is re-attached or made again), so
//! no tmux session of this host is consulted.

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{
    LaunchNode, TabEntry, AGENT_KIND, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY,
};

use super::{restore_plan, LiveTabs, TabRestore, RESUMABLE_PROPERTY};

const TAB: &str = "019b-target-tmux-tab";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-000000000002";
const DEV: &str = "dev";
const TAB_TMUX: &str = "tddy-019a-session-019b-tab";

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
fn a_tab_attached_to_a_tmux_session_on_a_target_is_planned_on_the_target() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::tmux_attach("work", DEV),
    ]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetTmux {
            session: "work".to_string()
        }
    );
}

#[test]
fn a_tab_attached_to_a_tmux_session_on_a_target_is_planned_whatever_this_hosts_tmux_holds() {
    // Given a tmux session of the same name on this host
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::tmux_attach("work", DEV),
    ]);
    let live = LiveTabs::none().with_tmux_session(DEV, "work");

    // When
    let plan = restore_plan(&tab, &live);

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetTmux {
            session: "work".to_string()
        }
    );
}

#[test]
fn an_agent_in_tmux_on_a_target_is_planned_resuming_its_conversation() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetTmuxAgent {
            session: TAB_TMUX.to_string(),
            harness: "claude-cli".to_string(),
            resume_conversation: Some(CONVERSATION.to_string()),
        }
    );
}

#[test]
fn an_agent_in_tmux_on_a_target_whose_harness_cannot_resume_is_planned_fresh() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::new(
            AGENT_KIND,
            [
                (HARNESS_PROPERTY, "opencode-cli"),
                (CONVERSATION_ID_PROPERTY, CONVERSATION),
                (RESUMABLE_PROPERTY, "false"),
            ],
        ),
    ]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetTmuxAgent {
            session: TAB_TMUX.to_string(),
            harness: "opencode-cli".to_string(),
            resume_conversation: None,
        }
    );
}

#[test]
fn a_tmux_attach_on_a_target_recording_no_session_is_dropped_naming_it() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::new("tmux-attach", [("os_user", DEV)]),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("session"), "{reason}");
}

#[test]
fn a_tmux_on_a_target_wrapping_no_agent_is_dropped() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::target(),
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::shell(),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("tmux"), "{reason}");
}
