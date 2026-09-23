//! Shared logic for merging streaming agent text into [`crate::presenter::state::PresenterState::activity_log`].
//! Used by [`super::presenter_impl::Presenter::poll_workflow`] and by remote views (VirtualTui) that
//! receive [`super::presenter_events::PresenterEvent::AgentOutput`] but do not run `poll_workflow`.

use super::agent_activity;
use super::state::{ActivityEntry, ActivityKind};

/// Incremental merge state for `WorkflowEvent::AgentOutput` / `PresenterEvent::AgentOutput` chunks.
#[derive(Debug, Default)]
pub struct AgentOutputActivityLogMerge {
    agent_output_buffer: String,
    agent_output_partial_row_active: bool,
}

impl AgentOutputActivityLogMerge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one backend/agent chunk the same way as [`super::presenter_impl::Presenter::poll_workflow`].
    pub fn apply_chunk(&mut self, text: &str, activity_log: &mut Vec<ActivityEntry>) {
        agent_activity::on_agent_chunk_received(text);
        for part in text.split_inclusive('\n') {
            if part.ends_with('\n') {
                self.agent_output_buffer
                    .push_str(part.trim_end_matches('\n'));
                let line = std::mem::take(&mut self.agent_output_buffer);
                if !line.is_empty() {
                    finalize_agent_line_in_activity_log(
                        activity_log,
                        &mut self.agent_output_partial_row_active,
                        line,
                    );
                }
            } else {
                self.agent_output_buffer.push_str(part);
            }
        }
        sync_agent_partial_activity_log(
            activity_log,
            &mut self.agent_output_partial_row_active,
            &self.agent_output_buffer,
        );
    }

    /// Flush trailing buffer without a newline (e.g. before workflow completion / tool interrupt).
    pub fn flush_buffer(&mut self, activity_log: &mut Vec<ActivityEntry>) {
        if self.agent_output_buffer.is_empty() {
            return;
        }
        let line = std::mem::take(&mut self.agent_output_buffer);
        log::debug!(
            "AgentOutputActivityLogMerge::flush_buffer: len={}, partial_row_active={}",
            line.len(),
            self.agent_output_partial_row_active
        );
        if self.agent_output_partial_row_active {
            if let Some(last) = activity_log.last() {
                if last.kind == ActivityKind::AgentOutput && last.text == line {
                    self.agent_output_partial_row_active = false;
                    return;
                }
            }
            self.agent_output_partial_row_active = false;
        }
        activity_log.push(ActivityEntry {
            text: line,
            kind: ActivityKind::AgentOutput,
        });
    }
}

fn finalize_agent_line_in_activity_log(
    activity_log: &mut Vec<ActivityEntry>,
    partial_row_active: &mut bool,
    line: String,
) {
    if line.is_empty() {
        return;
    }
    log::debug!(
        "finalize_agent_line_in_activity_log: len={}, partial_row_active={}",
        line.len(),
        partial_row_active
    );
    if *partial_row_active {
        if let Some(last) = activity_log.last_mut() {
            if last.kind == ActivityKind::AgentOutput {
                last.text = line;
                *partial_row_active = false;
                return;
            }
        }
        *partial_row_active = false;
    }
    activity_log.push(ActivityEntry {
        text: line,
        kind: ActivityKind::AgentOutput,
    });
}

fn sync_agent_partial_activity_log(
    activity_log: &mut Vec<ActivityEntry>,
    partial_row_active: &mut bool,
    agent_output_buffer: &str,
) {
    let tail = agent_activity::visible_tail_for_incremental_log(agent_output_buffer);
    if tail.is_empty() {
        return;
    }
    log::debug!(
        "sync_agent_partial_activity_log: tail_len={}, partial_row_active={}",
        tail.len(),
        partial_row_active
    );
    if *partial_row_active {
        if let Some(last) = activity_log.last_mut() {
            if last.kind == ActivityKind::AgentOutput {
                last.text = tail;
                return;
            }
        }
    }
    activity_log.push(ActivityEntry {
        text: tail,
        kind: ActivityKind::AgentOutput,
    });
    *partial_row_active = true;
}
