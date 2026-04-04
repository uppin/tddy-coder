//! Status bar worktree segment: view `PresenterState` must track `active_worktree_display` in lockstep
//! with the presenter when a worktree switch is broadcast.
//!
//! The presenter sets `PresenterState::active_worktree_display` from `format_worktree_for_status_bar`
//! and emits [`PresenterEvent::ActivityLogged`] with `Worktree: <path>`. The TUI keeps a clone of
//! state updated only via [`tddy_tui::apply_event`], so the same information must update
//! `active_worktree_display` there; otherwise the status bar never shows the segment after attach.

use std::time::Instant;

use tddy_core::{
    ActivityEntry, ActivityKind, AgentOutputActivityLogMerge, AppMode, PresenterEvent,
    PresenterState,
};
use tddy_tui::{apply_event, TuiView};

fn base_state() -> PresenterState {
    PresenterState {
        agent: "stub".to_string(),
        model: "stub".to_string(),
        mode: AppMode::Running,
        current_goal: Some("acceptance-tests".to_string()),
        current_state: Some("RunningAcceptanceTests".to_string()),
        workflow_session_id: None,
        goal_start_time: Instant::now(),
        activity_log: Vec::new(),
        inbox: Vec::new(),
        should_quit: false,
        exit_action: None,
        plan_refinement_pending: false,
        skills_project_root: None,
        active_worktree_display: None,
    }
}

/// Basename for this path matches what `format_worktree_for_status_bar` returns (final component).
#[test]
fn worktree_switch_broadcast_keeps_status_bar_segment_in_view_state() {
    let mut state = base_state();
    let mut view = TuiView::new();
    let mut merge = AgentOutputActivityLogMerge::new();

    let worktree_path = std::path::Path::new("/tmp/tddy-worktree-sync-verify/my-wt-dir");
    let entry = ActivityEntry {
        text: format!("Worktree: {}", worktree_path.display()),
        kind: ActivityKind::Info,
    };

    apply_event(
        &mut state,
        &mut view,
        &mut merge,
        PresenterEvent::ActivityLogged(entry),
    );

    assert_eq!(
        state.active_worktree_display.as_deref(),
        Some("my-wt-dir"),
        "view state must set active_worktree_display to the same short label the presenter uses for the status bar"
    );
    assert_eq!(state.activity_log.len(), 1);
}
