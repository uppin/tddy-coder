//! User-authored prompts in the activity log (queued lines keep a stable prefix).

/// Stable prefix for inbox queue lines.
pub const QUEUED_PROMPT_ACTIVITY_PREFIX: &str = "Queued: ";

/// Formats a submitted feature prompt for `activity_log` / `ActivityLogged` (plain text, no prefix).
#[must_use]
pub fn format_user_prompt_line(user_text: &str) -> String {
    log::info!(
        "activity_prompt_log: format_user_prompt_line (non-empty len={})",
        user_text.len()
    );
    user_text.to_string()
}

/// Formats a queued inbox prompt for `activity_log` / `ActivityLogged`.
#[must_use]
pub fn format_queued_prompt_line(queued_text: &str) -> String {
    log::info!(
        "activity_prompt_log: format_queued_prompt_line (len={})",
        queued_text.len()
    );
    log::debug!(
        "activity_prompt_log: queued prompt line prefix={:?}",
        QUEUED_PROMPT_ACTIVITY_PREFIX
    );
    format!("{}{}", QUEUED_PROMPT_ACTIVITY_PREFIX, queued_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_user_prompt_line_returns_submitted_text() {
        let text = "Build auth for unit test";
        let got = format_user_prompt_line(text);
        assert_eq!(got, text);
    }

    #[test]
    fn format_queued_prompt_line_includes_stable_prefix_and_text() {
        let text = "queued follow-up";
        let got = format_queued_prompt_line(text);
        let expected = format!("{}{}", QUEUED_PROMPT_ACTIVITY_PREFIX, text);
        assert_eq!(
            got, expected,
            "PRD contract: queued prompts must log with prefix {:?}",
            QUEUED_PROMPT_ACTIVITY_PREFIX
        );
    }
}
