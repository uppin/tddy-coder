//! Unit tests: the restore rule ([`restore_plan`]) for an agent tab recorded in a jail of its own
//! (`sandbox` → `agent`, `#session-liveness` 7/8): planned into a new jail, resuming its conversation
//! where its harness can, fresh where it cannot; dropped naming what it is missing.

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{LaunchNode, TabEntry, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY};

use super::{agent_launch_node, restore_plan, LiveTabs, TabRestore};

const TAB: &str = "019b-jailed-agent-tab";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-00000000a6e1";

fn a_jailed_tab(agent: LaunchNode) -> TabEntry {
    TabEntry::new(TAB, vec![LaunchNode::sandbox(), agent])
}

/// The reason of a dropped tab, failing the test when the tab is not dropped.
fn dropped_reason(plan: TabRestore) -> String {
    let TabRestore::Drop { reason } = plan else {
        panic!("expected the tab to be dropped, got {plan:?}");
    };
    reason
}

/// An `agent` node of `harness` in `CONVERSATION` without the property `key`.
fn an_agent_node_without(harness: &str, key: &str) -> LaunchNode {
    let mut node = LaunchNode::agent(harness, CONVERSATION);
    node.properties.remove(key);
    node
}

#[test]
fn a_claude_tab_recorded_in_a_sandbox_is_planned_into_a_new_jail_resuming_its_conversation() {
    // Given
    let tab = a_jailed_tab(agent_launch_node("claude-cli", CONVERSATION, true));

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::JailedAgent {
            harness: "claude-cli".to_string(),
            resume_conversation: Some(CONVERSATION.to_string()),
        }
    );
}

#[test]
fn an_opencode_tab_recorded_in_a_sandbox_is_planned_into_a_new_jail_fresh() {
    // Given an agent tab its harness cannot resume
    let tab = a_jailed_tab(agent_launch_node("opencode-cli", CONVERSATION, false));

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::JailedAgent {
            harness: "opencode-cli".to_string(),
            resume_conversation: None,
        }
    );
}

#[test]
fn a_jailed_agent_tab_recording_no_harness_is_dropped_naming_it() {
    // Given
    let tab = a_jailed_tab(an_agent_node_without("claude-cli", HARNESS_PROPERTY));

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains(HARNESS_PROPERTY), "{reason}");
}

#[test]
fn a_resumable_jailed_agent_tab_recording_no_conversation_is_dropped_naming_it() {
    // Given
    let tab = a_jailed_tab(an_agent_node_without(
        "claude-cli",
        CONVERSATION_ID_PROPERTY,
    ));

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains(CONVERSATION_ID_PROPERTY), "{reason}");
}

#[test]
fn a_jailed_agent_wrapping_anything_more_is_dropped_naming_the_sandbox() {
    // Given a jail whose agent is recorded wrapping a further node
    let tab = TabEntry::new(
        TAB,
        vec![
            LaunchNode::sandbox(),
            LaunchNode::agent("claude-cli", CONVERSATION),
            LaunchNode::shell(),
        ],
    );

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("sandbox"), "{reason}");
}
