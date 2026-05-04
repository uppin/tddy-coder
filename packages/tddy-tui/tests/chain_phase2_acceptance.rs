//! Phase 2 TUI chain parity (PRD Testing Plan). Lives in `tests/` so `include_str!` of `view_state.rs`
//! does not accidentally match test-only strings embedded in the same file.

/// **tui_chain_parent_pick_and_bootstrap_parity** — TUI must implement parent session picker and
/// worktree/bootstrap flow aligned with Telegram `/chain-workflow` (marker in production sources).
#[test]
fn tui_chain_parent_pick_and_bootstrap_parity() {
    let view_state = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/view_state.rs"));
    assert!(
        view_state.contains("chain_workflow_parent_picker"),
        "expected chain_workflow_parent_picker wiring in view_state.rs (TUI parent pick + bootstrap parity)"
    );
    assert!(
        tddy_tui::view_state::session_chaining_phase2_tui_chain_parity_ready(),
        "Phase 2 GREEN: flip session_chaining_phase2_tui_chain_parity_ready when TUI parent pick matches Telegram /chain-workflow"
    );
}
