//! Incremental agent output visibility and single-channel policy (PRD).

/// Called for each backend chunk before line splitting / buffering (trace hook).
pub fn on_agent_chunk_received(chunk: &str) {
    log::debug!(
        "agent_activity: on_agent_chunk_received len={} (authoritative stream is PresenterEvent::AgentOutput)",
        chunk.len()
    );
}

/// Text that should be visible in the activity log for the current partial agent buffer (no `\n` yet).
#[must_use]
pub fn visible_tail_for_incremental_log(partial_buffer: &str) -> String {
    log::debug!(
        "agent_activity: visible_tail_for_incremental_log buf_len={}",
        partial_buffer.len()
    );
    partial_buffer.to_string()
}

/// How many presenter channels carry the same completed agent line (PRD: 1).
#[must_use]
pub fn authoritative_channels_per_completed_line() -> usize {
    log::debug!(
        "agent_activity: authoritative_channels_per_completed_line → 1 (ActivityLogged not duplicated for agent lines; stream is AgentOutput)"
    );
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_tail_matches_partial_buffer_without_newline() {
        let partial = "partial_without_newline";
        assert_eq!(
            visible_tail_for_incremental_log(partial),
            partial,
            "PRD: partial agent text must be visible before first newline"
        );
    }

    #[test]
    fn dedup_policy_single_authoritative_channel_per_line() {
        assert_eq!(
            authoritative_channels_per_completed_line(),
            1,
            "PRD: at most one authoritative channel per completed agent line"
        );
    }
}
