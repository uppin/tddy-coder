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
    // TODO(agent-notify): implement
    let _ = (title, body);
    Err("TODO(agent-notify): validate_agent_notification is not implemented".to_string())
}
