//! Status bar activity: agent-active vs user-question wait, frozen elapsed, idle dot pulse, VirtualTui cadence.
//!
//! Idle dot uses middle dot U+00B7 (`·`) and bullet U+2022 (`•`) — readable on 80×24 monospace (PRD).

use std::time::Duration;

use tddy_core::{AppMode, PresenterState};

use crate::view_state::ViewState;

/// `true` when the workflow should show the **fast spinner** and **live** goal elapsed.
///
/// Agent-active is [`AppMode::Running`]. Clarification waits (`Select` / `MultiSelect` / `TextInput`)
/// use a frozen clock and 1 Hz idle dot. Other modes (FeatureInput, DocumentReview, …) follow product
/// alignment in a later phase; they are not `Running`, so they use the idle status treatment.
pub fn status_activity_is_agent_active(mode: &AppMode) -> bool {
    let active = matches!(mode, AppMode::Running);
    log::trace!(
        "status_bar_activity: agent_active={} (mode={:?})",
        active,
        mode
    );
    active
}

/// Elapsed duration shown in the `Goal:` row (third `│` segment).
pub fn display_elapsed_for_goal_row(state: &PresenterState, view_state: &ViewState) -> Duration {
    let user_wait = matches!(
        &state.mode,
        AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. }
    );
    if user_wait {
        if let Some(frozen) = view_state.frozen_goal_elapsed_for_status_bar {
            log::trace!(
                "status_bar_activity: display frozen goal elapsed {:?}",
                frozen
            );
            return frozen;
        }
        log::debug!("status_bar_activity: user-wait without freeze snapshot — using live elapsed");
    }
    let live = state.goal_start_time.elapsed();
    log::trace!("status_bar_activity: live goal elapsed {:?}", live);
    live
}

/// Leading activity glyph: fast spinner (`SPINNER_FRAMES`) when agent-active; 1 Hz · / • in user-wait.
pub fn activity_prefix_char_for_draw(
    mode: &AppMode,
    view_state: &ViewState,
    spinner_frames: &[char],
) -> char {
    if matches!(mode, AppMode::Running) {
        let c = spinner_frames[(view_state.spinner_tick / 4) % spinner_frames.len()];
        log::trace!("status_bar_activity: spinner frame {:?}", c);
        return c;
    }
    let secs = view_state
        .idle_dot_animation_anchor
        .map(|a| a.elapsed().as_secs())
        .unwrap_or(0);
    let c = if secs.is_multiple_of(2) { '·' } else { '•' };
    log::trace!("status_bar_activity: idle dot secs={} glyph={:?}", secs, c);
    c
}

/// VirtualTui autonomous re-render interval: fast while agent-active animations run; ~1 Hz in clarification wait.
pub fn virtual_tui_periodic_render_interval(mode: &AppMode) -> Duration {
    if matches!(
        mode,
        AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. }
    ) {
        log::trace!(
            "status_bar_activity: VirtualTui periodic interval 1000ms (clarification wait)"
        );
        Duration::from_secs(1)
    } else {
        log::trace!("status_bar_activity: VirtualTui periodic interval 200ms (agent-active)");
        Duration::from_millis(200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tddy_core::{AppMode, ClarificationQuestion, QuestionOption};

    fn select_mode() -> AppMode {
        AppMode::Select {
            question: ClarificationQuestion {
                header: "h".to_string(),
                question: "q".to_string(),
                options: vec![QuestionOption {
                    label: "a".to_string(),
                    description: String::new(),
                }],
                multi_select: false,
                allow_other: false,
            },
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        }
    }

    #[test]
    fn prd_select_mode_is_not_agent_active_for_status() {
        assert!(
            !status_activity_is_agent_active(&select_mode()),
            "Select must be classified as user-wait (false)"
        );
    }

    #[test]
    fn prd_virtual_tui_periodic_interval_is_at_least_one_second_in_select_wait() {
        assert!(
            virtual_tui_periodic_render_interval(&select_mode()) >= Duration::from_millis(900),
            "idle clarification wait should use ~1s periodic tick"
        );
    }
}
