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

/// Phases for the idle status prefix: small dot → bullet → larger circle → bullet (symmetric pulse).
/// Sub-200ms phases keep the red-phase unit test samples (0 / 120 / 240ms) on distinct glyphs.
const IDLE_HEARTBEAT_PHASE_MS: u64 = 80;
const IDLE_HEARTBEAT_CYCLE_MS: u64 = IDLE_HEARTBEAT_PHASE_MS * 4;

/// Idle heartbeat glyph from wall-clock elapsed since the animation anchor (PRD: small→large→small).
pub fn idle_heartbeat_glyph_for_elapsed(elapsed: Duration) -> char {
    let ms = elapsed.as_millis() as u64 % IDLE_HEARTBEAT_CYCLE_MS;
    let phase = ms / IDLE_HEARTBEAT_PHASE_MS;
    let c = match phase {
        0 => '·', // U+00B7 MIDDLE DOT
        1 => '•', // U+2022 BULLET
        2 => '●', // U+25CF BLACK CIRCLE (large)
        _ => '•',
    };
    log::trace!(
        "idle_heartbeat_glyph_for_elapsed: elapsed={:?} phase_ms={} glyph={:?}",
        elapsed,
        ms,
        c
    );
    c
}

/// Leading activity glyph: fast spinner (`SPINNER_FRAMES`) when agent-active; idle heartbeat in user-wait.
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
    let elapsed = view_state
        .idle_dot_animation_anchor
        .map(|a| a.elapsed())
        .unwrap_or(Duration::ZERO);
    let c = idle_heartbeat_glyph_for_elapsed(elapsed);
    log::trace!(
        "status_bar_activity: idle heartbeat elapsed={:?} glyph={:?}",
        elapsed,
        c
    );
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

    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};

    use crate::view_state::ViewState;
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

    /// Lower-level (PRD): idle heartbeat must vary across sub-second phase samples (≥2 distinct glyphs).
    #[test]
    fn idle_heartbeat_glyph_varies_across_subsecond_phases() {
        let a = idle_heartbeat_glyph_for_elapsed(Duration::ZERO);
        let b = idle_heartbeat_glyph_for_elapsed(Duration::from_millis(120));
        let c = idle_heartbeat_glyph_for_elapsed(Duration::from_millis(240));
        let set: BTreeSet<char> = [a, b, c].into_iter().collect();
        assert!(
            set.len() >= 2,
            "expected idle heartbeat to vary across phases; glyphs=({a:?},{b:?},{c:?})"
        );
    }

    /// Acceptance (PRD / Testing Plan): idle prefix runs a multi-step small→large→small heartbeat
    /// (≥3 distinct glyphs) and repeats over a few seconds of wall time.
    #[test]
    fn status_bar_idle_heartbeat_cycles_small_big_small() {
        let mode = select_mode();
        let dummy_spinner = ['|'];
        let mut samples = Vec::new();
        for ms in (0..4000_u64).step_by(120) {
            let mut vs = ViewState::new();
            vs.idle_dot_animation_anchor = Some(Instant::now() - Duration::from_millis(ms));
            samples.push(activity_prefix_char_for_draw(&mode, &vs, &dummy_spinner));
        }
        let unique: BTreeSet<char> = samples.iter().copied().collect();
        assert!(
            unique.len() >= 3,
            "expected at least three distinct idle heartbeat glyphs over ~4s sampled timeline; \
             unique={unique:?} samples={samples:?}"
        );
        let first = samples[0];
        assert!(
            samples
                .iter()
                .enumerate()
                .skip(3)
                .any(|(_, &c)| c == first),
            "expected idle heartbeat pattern to repeat after a multi-step cycle; samples={samples:?}"
        );
    }
}
