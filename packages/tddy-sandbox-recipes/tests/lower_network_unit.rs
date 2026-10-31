//! Unit: lowering a tool set's `network:` sections into one jail's egress policy — the union rule.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md` (`#agent-tools 3/12`).

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{Egress, HostPattern, NetworkSection, ResolvedTool};
use tddy_sandbox::EgressPolicy;
use tddy_sandbox_recipes::lower_network;

fn a_tool(id: &str, egress: Egress) -> ResolvedTool {
    ResolvedTool {
        id: id.into(),
        command: id.into(),
        binary: PathBuf::from(format!("/usr/bin/{id}")),
        reads: vec![],
        network: NetworkSection {
            egress,
            oauth_inbound: false,
        },
    }
}

fn taking_an_oauth_callback(mut tool: ResolvedTool) -> ResolvedTool {
    tool.network.oauth_inbound = true;
    tool
}

fn hosts(patterns: &[&str]) -> Egress {
    Egress::Hosts(
        patterns
            .iter()
            .map(|p| HostPattern::parse(p).expect("a valid pattern"))
            .collect(),
    )
}

fn a_policy_of(patterns: &[(&str, u16)]) -> EgressPolicy {
    EgressPolicy::Hosts(
        patterns
            .iter()
            .map(|(host, port)| tddy_sandbox::HostPattern::new(*host, *port))
            .collect(),
    )
}

#[test]
fn a_set_of_only_none_tools_lowers_to_none() {
    // Given
    let tools = [a_tool("rg", Egress::None), a_tool("jq", Egress::None)];

    // When / Then
    assert_eq!(lower_network(&tools), (EgressPolicy::None, false));
}

#[test]
fn an_empty_tool_set_lowers_to_none() {
    assert_eq!(lower_network(&[]), (EgressPolicy::None, false));
}

#[test]
fn one_any_tool_makes_the_set_any_whatever_the_others_declare() {
    // Given
    let tools = [
        a_tool("rg", Egress::None),
        a_tool("gh", hosts(&["api.github.com"])),
        a_tool("curl", Egress::Any),
    ];

    // When / Then
    assert_eq!(lower_network(&tools), (EgressPolicy::Any, false));
}

#[test]
fn host_lists_concatenate_in_tool_order_and_none_contributes_nothing() {
    // Given
    let tools = [
        a_tool(
            "gh",
            hosts(&["api.github.com", "*.githubusercontent.com:443"]),
        ),
        a_tool("rg", Egress::None),
        a_tool("cargo", hosts(&["index.crates.io", "127.0.0.1:3080"])),
    ];

    // When / Then
    assert_eq!(
        lower_network(&tools),
        (
            a_policy_of(&[
                ("api.github.com", 443),
                ("*.githubusercontent.com", 443),
                ("index.crates.io", 443),
                ("127.0.0.1", 3080),
            ]),
            false
        )
    );
}

#[test]
fn a_host_two_tools_declare_appears_once() {
    // Given
    let tools = [
        a_tool("gh", hosts(&["api.github.com"])),
        a_tool("git", hosts(&["API.github.com:443"])),
    ];

    // When / Then
    assert_eq!(
        lower_network(&tools),
        (a_policy_of(&[("api.github.com", 443)]), false)
    );
}

#[test]
fn oauth_inbound_is_true_when_any_tool_declares_it() {
    // Given
    let tools = [
        a_tool("rg", Egress::None),
        taking_an_oauth_callback(a_tool("login", hosts(&["auth.example.com"]))),
    ];

    // When / Then
    assert_eq!(
        lower_network(&tools),
        (a_policy_of(&[("auth.example.com", 443)]), true)
    );
}

#[test]
fn oauth_inbound_stands_even_when_no_tool_reaches_the_network() {
    // Given — a callback is inbound; it needs no egress of its own
    let tools = [taking_an_oauth_callback(a_tool("login", Egress::None))];

    // When / Then
    assert_eq!(lower_network(&tools), (EgressPolicy::None, true));
}
