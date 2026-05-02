//! Granular Phase 2 TUI chain parity tests (GREEN).

#[test]
fn chain_workflow_parent_picker_state_reports_ready_in_green() {
    let mut vs = tddy_tui::ViewState::new();
    vs.chain_workflow_parent_picker_active = true;
    assert!(
        vs.chain_workflow_parent_picker_state(),
        "GREEN: chain_workflow_parent_picker_state reflects active parent-picker"
    );
}

#[test]
fn tui_chain_parity_gate_is_false_until_green() {
    assert!(
        tddy_tui::view_state::session_chaining_phase2_tui_chain_parity_ready(),
        "GREEN: session_chaining_phase2_tui_chain_parity_ready when TUI matches Telegram /chain-workflow"
    );
}
