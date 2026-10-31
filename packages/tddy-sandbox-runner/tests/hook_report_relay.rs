//! The in-jail tool-IPC socket relays a jailed agent's hook reports to the facilitating daemon
//! (`#agent-tools 7/12`, PRD `docs/ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md`): a
//! jail is handed no host socket path, so the bridge is how `tddy-tools session-hook` reaches the
//! daemon from inside one.

use tddy_sandbox_runner::relays_to_the_host;
use tddy_service::session_activity::{ACTIVITY_SERVICE, IN_JAIL_RELAYABLE_HOOK_REPORTS};

#[test]
fn both_hook_reports_are_relayed_to_the_host() {
    // When
    let relayed: Vec<bool> = IN_JAIL_RELAYABLE_HOOK_REPORTS
        .iter()
        .map(|(service, method)| relays_to_the_host(service, method))
        .collect();

    // Then
    assert_eq!(relayed, vec![true, true]);
}

#[test]
fn the_roster_rpcs_are_still_relayed_beside_them() {
    // When
    let relayed: Vec<bool> = tddy_service::session_agents::IN_JAIL_RELAYABLE
        .iter()
        .map(|(service, method)| relays_to_the_host(service, method))
        .collect();

    // Then
    assert_eq!(relayed, vec![true; 6]);
}

#[test]
fn an_activity_rpc_that_is_not_a_hook_report_is_not_relayed() {
    // When — `AwaitQuestionAnswer` waits on an orchestrator; no Cursor hook needs it from a jail
    let relayed = relays_to_the_host(ACTIVITY_SERVICE, "AwaitQuestionAnswer");

    // Then
    assert!(
        !relayed,
        "AwaitQuestionAnswer must not be relayed from a jail"
    );
}
