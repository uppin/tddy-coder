//! `TerminalKind::from_wrapped_request` for the tmux tabs of a target session (`#target-tmux` 2/2):
//! a tmux attach or a new tmux session wrapped in `target` runs on the target, and an agent wrapped
//! in `[target, tmux]` runs in tmux on the target. A target around a tmux wrapper around anything but
//! an agent is still refused.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md (n2)

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::{TerminalKind, TARGET_WRAPPER_KIND, TMUX_WRAPPER_KIND};

fn wrapped_in(kinds: &[&str]) -> Vec<LaunchNode> {
    kinds
        .iter()
        .map(|kind| LaunchNode {
            kind: kind.to_string(),
            properties: HashMap::new(),
        })
        .collect()
}

#[test]
fn a_tmux_attach_wrapped_in_target_attaches_on_the_target() {
    // When
    let kind =
        TerminalKind::from_wrapped_request("tmux", "work", &wrapped_in(&[TARGET_WRAPPER_KIND]))
            .expect("a tmux attach on the target");

    // Then
    assert_eq!(
        kind,
        TerminalKind::TargetTmux {
            session: "work".to_string()
        }
    );
}

#[test]
fn a_new_tmux_session_wrapped_in_target_starts_on_the_target() {
    // When
    let kind = TerminalKind::from_wrapped_request(
        "tmux-new",
        "scratch",
        &wrapped_in(&[TARGET_WRAPPER_KIND]),
    )
    .expect("a new tmux session on the target");

    // Then
    assert_eq!(
        kind,
        TerminalKind::TargetTmuxNew {
            session: "scratch".to_string()
        }
    );
}

#[test]
fn an_agent_wrapped_in_target_then_tmux_runs_in_tmux_on_the_target() {
    // When
    let kind = TerminalKind::from_wrapped_request(
        " claude-cli ",
        "",
        &wrapped_in(&[TARGET_WRAPPER_KIND, TMUX_WRAPPER_KIND]),
    )
    .expect("an agent in tmux on the target");

    // Then
    assert_eq!(
        kind,
        TerminalKind::TargetTmuxAgent {
            agent: "claude-cli".to_string()
        }
    );
}

#[test]
fn a_tmux_attach_on_the_target_without_a_session_is_refused() {
    // When
    let refusal =
        TerminalKind::from_wrapped_request("tmux", " ", &wrapped_in(&[TARGET_WRAPPER_KIND]))
            .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn an_agent_in_tmux_on_a_target_wrapping_anything_more_is_refused_naming_it() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &wrapped_in(&[TARGET_WRAPPER_KIND, TMUX_WRAPPER_KIND, "sandbox"]),
    )
    .unwrap_err();

    // Then
    assert_eq!(
        (refusal.code(), refusal.message().contains("\"sandbox\"")),
        (Code::InvalidArgument, true),
        "the refusal: {refusal:?}"
    );
}
