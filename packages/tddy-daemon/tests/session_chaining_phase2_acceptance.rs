//! Phase 2 session chaining acceptance (PRD Testing Plan §2–4).
//!
//! **RED**: these tests assert production wiring exists in `telegram_bot` / `telegram_session_control`.
//! They fail until live `tcp:` dispatch, chain integration-base merge, and explicit-operator merge land.

fn telegram_bot_rs() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/telegram_bot.rs"))
}

fn telegram_session_control_rs() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/telegram_session_control.rs"
    ))
}

/// **live_telegram_bot_dispatches_tcp_chain_parent_callback** — long-polling `telegram_callback_handler`
/// must parse `tcp:<idx>|s:<child_uuid>`, authorize the chat, resolve the child session directory under
/// `sessions_base`, and call [`TelegramSessionControlHarness::handle_chain_parent_callback`] (same as harness).
#[test]
fn live_telegram_bot_dispatches_tcp_chain_parent_callback() {
    let src = telegram_bot_rs();
    assert!(
        src.contains("parse_telegram_chain_parent_callback")
            && src.contains("handle_chain_parent_callback"),
        "telegram_callback_handler must route tcp: callbacks through parse_telegram_chain_parent_callback \
         and invoke handle_chain_parent_callback on the harness (PRD: live Telegram tcp: routing)"
    );
    assert!(
        tddy_daemon::telegram_bot::session_chaining_phase2_live_tcp_dispatch_ready(),
        "Phase 2 GREEN: live tcp: dispatch must flip session_chaining_phase2_live_tcp_dispatch_ready when end-to-end wiring is verified"
    );
}

/// **telegram_chain_child_persists_parent_chain_base_on_default_flow** — when `.session.yaml` has
/// `previous_session_id`, the project → integration-base → spawn pipeline must apply
/// `resolve_chain_integration_base_ref_from_parent_session` / `integrate_chain_base_into_session_worktree_bootstrap`
/// so default flows match [`session_chain_acceptance`].
#[test]
fn telegram_chain_child_persists_parent_chain_base_on_default_flow() {
    let src = telegram_session_control_rs();
    assert!(
        src.contains("integrate_chain_base_into_session_worktree_bootstrap")
            || src.contains("resolve_chain_integration_base_ref_from_parent_session"),
        "telegram_session_control must wire tddy-core chain base helpers for chained children \
         (PRD: default integration-base consistent with parent session)"
    );
    assert!(
        tddy_daemon::telegram_session_control::session_chaining_phase2_chain_base_merge_ready(),
        "Phase 2 GREEN: flip session_chaining_phase2_chain_base_merge_ready when default-flow chain base wiring is complete"
    );
}

/// **telegram_chain_explicit_branch_choice_not_silently_dropped** — explicit operator selections for
/// integration base / branch intent must remain authoritative per documented merge rules when a parent
/// chain applies.
#[test]
fn telegram_chain_explicit_branch_choice_not_silently_dropped() {
    let src = telegram_session_control_rs();
    assert!(
        src.contains("merge_chain_integration_base_with_explicit_operator_overrides"),
        "Implement documented merge of parent-derived chain base with explicit operator overrides; \
         expected symbol merge_chain_integration_base_with_explicit_operator_overrides (or keep this \
         assertion in sync with the chosen API name)"
    );
    assert!(
        tddy_daemon::telegram_session_control::merge_chain_integration_base_with_explicit_operator_overrides(
            std::path::Path::new("/nonexistent-sessions-root"),
            "parent-id",
            std::path::Path::new("/nonexistent-child"),
            std::path::Path::new("/nonexistent-repo"),
            Some("origin/explicit-override"),
        )
        .is_ok(),
        "Phase 2 GREEN: merge must succeed for valid fixtures when explicit overrides are honored"
    );
}
