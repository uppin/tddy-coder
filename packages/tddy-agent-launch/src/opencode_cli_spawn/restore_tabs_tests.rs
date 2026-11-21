//! Unit tests: the per-entry restore rule ([`restore_plan`]) for every launch-chain kind, and the
//! agent-tab argv with and without `resume`.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::tab_roster::{
    LaunchNode, TabEntry, AGENT_KIND, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY,
};

use super::{agent_tab_argv, restore_plan, LiveTabs, TabRestore, RESUMABLE_PROPERTY};

const TAB: &str = "019b-tab";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-000000000001";
const DEV: &str = "dev";
const TAB_TMUX: &str = "tddy-019a-session-019b-tab";

/// An agent node of `harness` in [`CONVERSATION`], marked as its harness cannot resume it.
fn a_non_resumable_agent(harness: &str) -> LaunchNode {
    LaunchNode::new(
        AGENT_KIND,
        [
            (HARNESS_PROPERTY, harness),
            (CONVERSATION_ID_PROPERTY, CONVERSATION),
            (RESUMABLE_PROPERTY, "false"),
        ],
    )
}

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

// ---------------------------------------------------------------------------
// restore_plan
// ---------------------------------------------------------------------------

#[test]
fn a_tab_whose_terminal_is_live_is_left_as_it_is() {
    // Given
    let tab = a_tab(vec![LaunchNode::agent("claude-cli", CONVERSATION)]);
    let live = LiveTabs::none().with_terminal(TAB);

    // When
    let plan = restore_plan(&tab, &live);

    // Then
    assert_eq!(plan, TabRestore::LeaveLive);
}

#[test]
fn an_agent_tab_resumes_its_recorded_conversation() {
    // Given
    let tab = a_tab(vec![LaunchNode::agent("claude-cli", CONVERSATION)]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::ResumeAgent {
            harness: "claude-cli".to_string(),
            conversation_id: CONVERSATION.to_string(),
        }
    );
}

#[test]
fn an_agent_tab_marked_not_resumable_reopens_fresh() {
    // Given
    let tab = a_tab(vec![a_non_resumable_agent("opencode-cli")]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::FreshAgent {
            harness: "opencode-cli".to_string(),
        }
    );
}

#[test]
fn a_tmux_wrapped_agent_whose_tmux_session_lives_is_reattached() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);
    let live = LiveTabs::none().with_tmux_session(DEV, TAB_TMUX);

    // When
    let plan = restore_plan(&tab, &live);

    // Then
    assert_eq!(
        plan,
        TabRestore::ReattachTmux {
            session: TAB_TMUX.to_string(),
            os_user: DEV.to_string(),
        }
    );
}

#[test]
fn a_tmux_wrapped_agent_whose_tmux_session_is_gone_is_relaunched_resuming_its_conversation() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::RelaunchTmux {
            session: TAB_TMUX.to_string(),
            os_user: DEV.to_string(),
            harness: "claude-cli".to_string(),
            resume_conversation: Some(CONVERSATION.to_string()),
        }
    );
}

#[test]
fn a_tmux_wrapped_non_resumable_agent_whose_tmux_session_is_gone_is_relaunched_fresh() {
    // Given
    let tab = a_tab(vec![
        LaunchNode::tmux(TAB_TMUX, DEV),
        a_non_resumable_agent("muse-cli"),
    ]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(
        plan,
        TabRestore::RelaunchTmux {
            session: TAB_TMUX.to_string(),
            os_user: DEV.to_string(),
            harness: "muse-cli".to_string(),
            resume_conversation: None,
        }
    );
}

#[test]
fn a_tmux_session_on_another_users_server_does_not_count_as_the_tabs() {
    // Given a tab whose tmux session `dev` recorded, and a session of that name on another user's
    // server only
    let tab = a_tab(vec![
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);
    let live = LiveTabs::none().with_tmux_session("someone-else", TAB_TMUX);

    // When
    let plan = restore_plan(&tab, &live);

    // Then
    assert_eq!(
        plan,
        TabRestore::RelaunchTmux {
            session: TAB_TMUX.to_string(),
            os_user: DEV.to_string(),
            harness: "claude-cli".to_string(),
            resume_conversation: Some(CONVERSATION.to_string()),
        }
    );
}

#[test]
fn a_shell_tab_reopens_fresh() {
    // Given
    let tab = a_tab(vec![LaunchNode::shell()]);

    // When
    let plan = restore_plan(&tab, &LiveTabs::none());

    // Then
    assert_eq!(plan, TabRestore::FreshShell);
}

#[test]
fn a_tmux_attach_tab_whose_user_session_exists_is_reattached() {
    // Given
    let tab = a_tab(vec![LaunchNode::tmux_attach("work", DEV)]);
    let live = LiveTabs::none().with_tmux_session(DEV, "work");

    // When
    let plan = restore_plan(&tab, &live);

    // Then
    assert_eq!(
        plan,
        TabRestore::ReattachTmuxAttach {
            session: "work".to_string(),
            os_user: DEV.to_string(),
        }
    );
}

#[test]
fn a_tmux_attach_tab_whose_user_session_is_gone_is_dropped_naming_it() {
    // Given
    let tab = a_tab(vec![LaunchNode::tmux_attach("work", DEV)]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("\"work\""), "{reason}");
}

#[test]
fn a_tab_of_a_kind_this_daemon_does_not_know_is_dropped_naming_the_kind() {
    // Given a chain a later daemon wrote
    let tab = a_tab(vec![
        LaunchNode::new("ssh", [("host", "build-box")]),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains("ssh"), "{reason}");
}

#[test]
fn a_tab_whose_chain_holds_a_wrapper_this_daemon_does_not_know_is_dropped_naming_it() {
    // Given a tmux-wrapped agent a later daemon also wrapped in a jail
    let tab = a_tab(vec![
        LaunchNode::new("sandbox", [("profile", "default")]),
        LaunchNode::tmux(TAB_TMUX, DEV),
        LaunchNode::agent("claude-cli", CONVERSATION),
    ]);
    let live = LiveTabs::none().with_tmux_session(DEV, TAB_TMUX);

    // When
    let reason = dropped_reason(restore_plan(&tab, &live));

    // Then
    assert!(reason.contains("sandbox"), "{reason}");
}

#[test]
fn a_tab_with_no_launch_chain_is_dropped() {
    // Given
    let tab = a_tab(Vec::new());

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(!reason.is_empty());
}

#[test]
fn an_agent_tab_that_records_no_harness_is_dropped_naming_the_missing_property() {
    // Given
    let tab = a_tab(vec![LaunchNode::new(
        AGENT_KIND,
        [(CONVERSATION_ID_PROPERTY, CONVERSATION)],
    )]);

    // When
    let reason = dropped_reason(restore_plan(&tab, &LiveTabs::none()));

    // Then
    assert!(reason.contains(HARNESS_PROPERTY), "{reason}");
}

// ---------------------------------------------------------------------------
// agent_tab_argv
// ---------------------------------------------------------------------------

/// The value following `flag` in `argv`.
fn value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

fn the_builtin_spec_of(session_type: &str) -> &'static tddy_coding_agent_specs::CodingAgentSpec {
    CodingAgentRegistry::builtin_cached()
        .for_session_type(session_type)
        .unwrap_or_else(|| panic!("{session_type} is a built-in coding agent"))
}

#[test]
fn a_claude_tab_resumed_renders_resume_with_its_conversation() {
    // Given
    let claude = the_builtin_spec_of("claude-cli");

    // When
    let argv = agent_tab_argv(claude, Path::new("/usr/bin/claude"), CONVERSATION, true)
        .expect("the claude tab argv renders");

    // Then
    assert_eq!(value_after(&argv, "--resume"), Some(CONVERSATION));
    assert_eq!(value_after(&argv, "--session-id"), None);
}

#[test]
fn a_claude_tab_started_renders_its_conversation_as_the_session_id() {
    // Given
    let claude = the_builtin_spec_of("claude-cli");

    // When
    let argv = agent_tab_argv(claude, Path::new("/usr/bin/claude"), CONVERSATION, false)
        .expect("the claude tab argv renders");

    // Then
    assert_eq!(value_after(&argv, "--session-id"), Some(CONVERSATION));
    assert_eq!(value_after(&argv, "--resume"), None);
}

#[test]
fn a_cursor_tab_resumed_renders_resume_with_its_chat() {
    // Given
    let cursor = the_builtin_spec_of("cursor-cli");

    // When
    let argv = agent_tab_argv(cursor, Path::new("/usr/bin/agent"), CONVERSATION, true)
        .expect("the cursor tab argv renders");

    // Then
    assert_eq!(value_after(&argv, "--resume"), Some(CONVERSATION));
}
