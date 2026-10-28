//! The control tokens a project-home session holds on this host's terminals are remembered per
//! confirmed caller and target, and nowhere else (`#home-inbox` 5/8).

use tddy_daemon_rpc::exec_tool::home_terminal_leases::{HomeTerminalClaimKey, HomeTerminalLeases};

fn a_claim_by(instance: &str, caller: &str, target: &str) -> HomeTerminalClaimKey {
    HomeTerminalClaimKey {
        caller_daemon_instance_id: instance.to_string(),
        caller_session_id: caller.to_string(),
        target_session_id: target.to_string(),
    }
}

fn the_claim() -> HomeTerminalClaimKey {
    a_claim_by("host-a", "home-1", "target-1")
}

#[test]
fn a_claim_that_was_remembered_gives_its_token_back() {
    // Given a home session that claimed a terminal and was issued a token
    let leases = HomeTerminalLeases::default();
    leases.remember(the_claim(), "token-1".to_string());

    // When the token is looked up for that claim
    let token = leases.token_of(&the_claim());

    // Then it is the one issued
    assert_eq!(token.as_deref(), Some("token-1"));
}

#[test]
fn a_claim_that_was_never_made_has_no_token() {
    // Given a store holding another claim only
    let leases = HomeTerminalLeases::default();
    leases.remember(the_claim(), "token-1".to_string());
    let never_claimed = a_claim_by("host-a", "home-1", "target-2");

    // Then the unclaimed target has none, and the claimed one still does
    assert_eq!(leases.token_of(&never_claimed), None);
    assert_eq!(leases.token_of(&the_claim()).as_deref(), Some("token-1"));
}

#[test]
fn a_new_claim_replaces_the_token_of_the_old_one() {
    // Given a claim, a steal by a person, and a claim again, which issues a new token
    let leases = HomeTerminalLeases::default();
    leases.remember(the_claim(), "token-1".to_string());
    leases.remember(the_claim(), "token-2".to_string());

    // Then the latest token is the one held
    assert_eq!(leases.token_of(&the_claim()).as_deref(), Some("token-2"));
}

#[test]
fn another_home_session_does_not_share_a_token() {
    let leases = HomeTerminalLeases::default();
    leases.remember(the_claim(), "token-1".to_string());

    assert_eq!(
        leases.token_of(&a_claim_by("host-a", "home-2", "target-1")),
        None
    );
    assert_eq!(leases.token_of(&the_claim()).as_deref(), Some("token-1"));
}

#[test]
fn a_home_session_on_another_host_does_not_share_a_token() {
    // Given a home session id that two hosts report: they are told apart by the owning daemon
    let leases = HomeTerminalLeases::default();
    leases.remember(the_claim(), "token-1".to_string());

    assert_eq!(
        leases.token_of(&a_claim_by("host-b", "home-1", "target-1")),
        None
    );
    assert_eq!(leases.token_of(&the_claim()).as_deref(), Some("token-1"));
}

#[test]
fn each_target_of_one_home_session_has_its_own_token() {
    // Given one home session that claimed two terminals
    let leases = HomeTerminalLeases::default();
    leases.remember(
        a_claim_by("host-a", "home-1", "target-1"),
        "token-1".to_string(),
    );
    leases.remember(
        a_claim_by("host-a", "home-1", "target-2"),
        "token-2".to_string(),
    );

    // Then each terminal's token is its own
    assert_eq!(
        leases
            .token_of(&a_claim_by("host-a", "home-1", "target-1"))
            .as_deref(),
        Some("token-1")
    );
    assert_eq!(
        leases
            .token_of(&a_claim_by("host-a", "home-1", "target-2"))
            .as_deref(),
        Some("token-2")
    );
}
