//! Error recovery: view state after workflow failure and presenter-driven exit.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tddy_core::{AppMode, PresenterEvent, PresenterState, UserIntent};
use tddy_tui::{apply_event, key_event_to_intent, TuiView, ViewState};

fn sample_state() -> PresenterState {
    PresenterState {
        agent: "cursor".to_string(),
        model: "opus".to_string(),
        mode: AppMode::Running,
        current_goal: Some("refactor".to_string()),
        current_state: Some("Refactoring".to_string()),
        goal_start_time: Instant::now(),
        activity_log: Vec::new(),
        inbox: Vec::new(),
        should_quit: false,
        exit_action: None,
    }
}

#[test]
fn workflow_complete_error_sets_error_recovery_mode() {
    let mut state = sample_state();
    let mut view = TuiView::new();
    let err = "read refactoring-plan.md: No such file or directory (os error 2)";
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::WorkflowComplete(Err(err.to_string())),
    );
    assert!(
        matches!(state.mode, AppMode::ErrorRecovery { ref error_message } if error_message == err),
        "expected ErrorRecovery with message, got {:?}",
        state.mode
    );
    assert!(
        !state.should_quit,
        "workflow error alone must not set should_quit on the view"
    );
}

#[test]
fn should_quit_event_exits_tui_loop_state() {
    let mut state = sample_state();
    let mut view = TuiView::new();
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::WorkflowComplete(Err("boom".into())),
    );
    apply_event(&mut state, &mut view, PresenterEvent::ShouldQuit);
    assert!(state.should_quit);
}

#[test]
fn workflow_error_preserves_goal_and_state_for_status_bar() {
    let mut state = sample_state();
    let mut view = TuiView::new();
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::WorkflowComplete(Err("read refactoring-plan.md: ...".into())),
    );
    assert_eq!(state.current_goal.as_deref(), Some("refactor"));
    assert_eq!(state.current_state.as_deref(), Some("Refactoring"));
}

#[test]
fn intent_received_quit_sets_should_quit_in_error_recovery() {
    let mut state = sample_state();
    state.mode = AppMode::ErrorRecovery {
        error_message: "read refactoring-plan.md: No such file or directory (os error 2)"
            .to_string(),
    };
    let mut view = TuiView::new();
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::IntentReceived(UserIntent::Quit),
    );
    assert!(
        state.should_quit,
        "Exit sends Quit; apply_event must set should_quit so the TUI loop can exit"
    );
}

#[test]
fn error_recovery_exit_selection_enter_maps_to_quit_intent() {
    let mut vs = ViewState::new();
    vs.error_recovery_selected = 2;
    let mode = AppMode::ErrorRecovery {
        error_message: "workflow failed".to_string(),
    };
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    let intent = key_event_to_intent(key, &mode, &vs);
    assert!(
        matches!(intent, Some(UserIntent::Quit)),
        "Exit is index 2; Enter must produce Quit, got {:?}",
        intent
    );
}

#[test]
fn workflow_error_then_quit_intent_exits() {
    let mut state = sample_state();
    let mut view = TuiView::new();
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::WorkflowComplete(Err("boom".into())),
    );
    apply_event(
        &mut state,
        &mut view,
        PresenterEvent::IntentReceived(UserIntent::Quit),
    );
    assert!(state.should_quit);
    assert!(
        matches!(state.mode, AppMode::ErrorRecovery { .. }),
        "mode stays ErrorRecovery until redraw; loop exits on should_quit"
    );
}
