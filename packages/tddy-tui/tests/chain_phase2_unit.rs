//! Granular Phase 2 TUI chain parity tests (GREEN).

#[test]
fn chain_workflow_parent_picker_state_reports_ready_when_active() {
    // Given
    let mut vs = tddy_tui::ViewState::new();
    vs.chain_workflow_parent_picker_active = true;

    // When / Then
    assert!(
        vs.chain_workflow_parent_picker_state(),
        "chain_workflow_parent_picker_state reflects active parent-picker"
    );
}

#[test]
fn tui_chain_parity_gate_is_ready() {
    // When / Then
    assert!(
        tddy_tui::view_state::session_chaining_phase2_tui_chain_parity_ready(),
        "session_chaining_phase2_tui_chain_parity_ready when TUI matches Telegram /chain-workflow"
    );
}
