//! How long an agent notification may be.
//!
//! Limits are refusals, never truncation: an agent whose message was cut would believe it had said
//! something it had not, while one that is refused learns its message was too long and can say it
//! again, shorter.

/// The longest title, in characters, after trimming.
pub const MAX_TITLE_CHARS: usize = 200;

/// The longest markdown body, in characters.
pub const MAX_BODY_CHARS: usize = 2_000;

/// `Ok` when `title` is 1–[`MAX_TITLE_CHARS`] characters after trimming and `body` is at most
/// [`MAX_BODY_CHARS`]; otherwise the reason, worded for the agent that sent it.
pub fn validate_agent_notification(title: &str, body: &str) -> Result<(), String> {
    let title_chars = title.trim().chars().count();
    if title_chars == 0 {
        return Err("the title is empty; give the notification a one-line title".to_string());
    }
    if title_chars > MAX_TITLE_CHARS {
        return Err(format!(
            "the title is {title_chars} characters; shorten it to at most {MAX_TITLE_CHARS}"
        ));
    }
    let body_chars = body.chars().count();
    if body_chars > MAX_BODY_CHARS {
        return Err(format!(
            "the body is {body_chars} characters; shorten it to at most {MAX_BODY_CHARS}"
        ));
    }
    Ok(())
}
