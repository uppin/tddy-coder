//! Unit tests: an agent tab on its session's target (`target` → `agent`, `#session-liveness` 8/8) —
//! the argv and environment it runs there ([`target_agent_tab`]: the spec's bare command, which the
//! target's login `PATH` resolves, never a path of this host), and the restore rule
//! ([`restore_plan`]): planned on the target, resuming its conversation where its harness can, fresh
//! where it cannot; dropped naming what it is missing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_core::tab_roster::{LaunchNode, TabEntry, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY};
use tddy_rpc::Code;

use super::{agent_launch_node, restore_plan, target_agent_tab, LiveTabs, TabRestore};
use crate::spec_launch::HostCodingAgents;

const TAB: &str = "019b-target-agent-tab";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-0000000a7a26";
const CHAT: &str = "c0ffee00-e154-41d0-ae72-0000000a7a26";

// ---------------------------------------------------------------------------
// What a target agent tab runs
// ---------------------------------------------------------------------------

/// A host on which no agent resolves: its environment and `PATH` are empty.
struct AHostWithoutAnyAgent;

impl ToolHost for AHostWithoutAnyAgent {
    fn env_var(&self, _name: &str) -> Option<String> {
        None
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, _path: &Path) -> bool {
        false
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }
}

/// The built-in coding agents on a daemon host that has none of them installed.
fn the_builtin_agents_on_a_host_without_any() -> HostCodingAgents {
    let registry = tddy_coding_agent_specs::load_agent_registry(Path::new("/nonexistent/tools"))
        .expect("the built-in coding agents load");
    HostCodingAgents::new(registry, Arc::new(AHostWithoutAnyAgent))
}

fn strings(argv: &[&str]) -> Vec<String> {
    argv.iter().map(|arg| arg.to_string()).collect()
}

#[test]
fn a_target_claude_tab_resumes_its_recorded_conversation_by_the_bare_command() {
    // Given
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let tab = target_agent_tab(&agents, "claude-cli", CONVERSATION, true)
        .expect("a Claude tab on the target");

    // Then
    assert_eq!(
        tab.argv,
        strings(&[
            "claude",
            "--resume",
            CONVERSATION,
            "--permission-mode",
            "auto"
        ])
    );
}

#[test]
fn a_new_target_claude_tab_starts_its_conversation_by_the_bare_command() {
    // Given
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let tab = target_agent_tab(&agents, "claude-cli", CONVERSATION, false)
        .expect("a Claude tab on the target");

    // Then
    assert_eq!(
        tab.argv,
        strings(&[
            "claude",
            "--session-id",
            CONVERSATION,
            "--permission-mode",
            "auto"
        ])
    );
}

#[test]
fn a_target_cursor_tab_resumes_its_chat_by_the_bare_command() {
    // Given
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let tab =
        target_agent_tab(&agents, "cursor-cli", CHAT, true).expect("a Cursor tab on the target");

    // Then the target's `agent` is run into the tab's chat
    assert_eq!(
        tab.argv[..3].to_vec(),
        strings(&["agent", "--resume", CHAT])
    );
}

#[test]
fn a_target_agent_tab_is_of_its_harness() {
    // Given
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let tab = target_agent_tab(&agents, "claude-cli", CONVERSATION, true)
        .expect("a Claude tab on the target");

    // Then
    assert_eq!(tab.kind, "claude-cli");
}

#[test]
fn a_target_agent_tab_carries_its_specs_run_environment() {
    // Given an agent whose spec sets a variable on its run (Claude's fullscreen renderer)
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let tab = target_agent_tab(&agents, "claude-cli", CONVERSATION, true)
        .expect("a Claude tab on the target");

    // Then the variable travels with the tab, since the target's launch starts from `env -i`
    assert!(
        tab.env
            .contains(&("CLAUDE_CODE_NO_FLICKER".to_string(), "1".to_string())),
        "{:?}",
        tab.env
    );
}

#[test]
fn a_target_tab_of_an_unknown_harness_is_refused_naming_it() {
    // Given
    let agents = the_builtin_agents_on_a_host_without_any();

    // When
    let refusal = target_agent_tab(&agents, "acme-cli", CONVERSATION, false)
        .expect_err("no agent runs acme-cli");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("acme-cli"),
        "{}",
        refusal.message()
    );
}

// ---------------------------------------------------------------------------
// Restoring a target agent tab
// ---------------------------------------------------------------------------

fn a_target_tab(agent: LaunchNode) -> TabEntry {
    TabEntry::new(TAB, vec![LaunchNode::target(), agent])
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
fn a_claude_tab_recorded_on_a_target_is_planned_on_the_target_resuming_its_conversation() {
    // Given
    let tab = a_target_tab(agent_launch_node("claude-cli", CONVERSATION, true));

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetAgent {
            harness: "claude-cli".to_string(),
            resume_conversation: Some(CONVERSATION.to_string()),
        }
    );
}

#[test]
fn an_opencode_tab_recorded_on_a_target_is_planned_on_the_target_fresh() {
    // Given an agent tab its harness cannot resume
    let tab = a_target_tab(agent_launch_node("opencode-cli", CONVERSATION, false));

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::TargetAgent {
            harness: "opencode-cli".to_string(),
            resume_conversation: None,
        }
    );
}

#[test]
fn a_target_agent_tab_recording_no_harness_is_dropped_naming_it() {
    // Given
    let tab = a_target_tab(an_agent_node_without("claude-cli", HARNESS_PROPERTY));

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains(HARNESS_PROPERTY), "{reason}");
}

#[test]
fn a_resumable_target_agent_tab_recording_no_conversation_is_dropped_naming_it() {
    // Given
    let tab = a_target_tab(an_agent_node_without(
        "claude-cli",
        CONVERSATION_ID_PROPERTY,
    ));

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains(CONVERSATION_ID_PROPERTY), "{reason}");
}

#[test]
fn a_target_agent_wrapping_anything_more_is_dropped_naming_the_target() {
    // Given a target whose agent is recorded wrapping a further node
    let tab = TabEntry::new(
        TAB,
        vec![
            LaunchNode::target(),
            LaunchNode::agent("claude-cli", CONVERSATION),
            LaunchNode::shell(),
        ],
    );

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("target"), "{reason}");
}
