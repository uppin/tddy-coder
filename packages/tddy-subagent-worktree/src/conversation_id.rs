//! The conversation id, checked before it becomes a directory name and a branch name.
//!
//! The caller chooses a conversation's id (`subagent_new_session { sessionId }`), and this crate
//! puts it in two places where an arbitrary string is dangerous: a path under the session worktree
//! and a ref name in the repository's common dir. So it is refused unless it is a plain component.

use std::fmt;

/// The longest id accepted — long enough for a UUID with a prefix, short enough for a path.
pub const CONVERSATION_ID_MAX_LEN: usize = 64;

/// A conversation id that is safe as one path component and one ref component:
/// `[A-Za-z0-9._-]{1,64}`, not starting with `.`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(String);

/// Why an id was refused, carrying the id as given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeConversationId(pub String);

impl ConversationId {
    /// Accept `raw` as a conversation id, or say why not.
    pub fn parse(raw: &str) -> Result<Self, UnsafeConversationId> {
        // TODO(isolated-edits): implement
        todo!("ConversationId::parse({raw:?})")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for UnsafeConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "conversation id {:?} is not usable as a worktree name: it must be 1-{CONVERSATION_ID_MAX_LEN} \
             characters of [A-Za-z0-9._-] and must not start with '.'",
            self.0
        )
    }
}

impl std::error::Error for UnsafeConversationId {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uuid_like_id_is_accepted() {
        // Given
        let raw = "conv-3f9c2ab0-1d2e.v2_final";

        // When
        let id = ConversationId::parse(raw);

        // Then
        assert_eq!(id.map(|id| id.as_str().to_string()), Ok(raw.to_string()));
    }

    #[test]
    fn an_id_with_a_slash_is_refused() {
        assert_eq!(
            ConversationId::parse("a/b"),
            Err(UnsafeConversationId("a/b".into()))
        );
    }

    #[test]
    fn a_parent_directory_id_is_refused() {
        assert_eq!(
            ConversationId::parse(".."),
            Err(UnsafeConversationId("..".into()))
        );
    }

    #[test]
    fn an_id_starting_with_a_dot_is_refused() {
        assert_eq!(
            ConversationId::parse(".hidden"),
            Err(UnsafeConversationId(".hidden".into()))
        );
    }

    #[test]
    fn an_empty_id_is_refused() {
        assert_eq!(
            ConversationId::parse(""),
            Err(UnsafeConversationId(String::new()))
        );
    }

    #[test]
    fn an_id_one_past_the_length_limit_is_refused() {
        let raw = "a".repeat(CONVERSATION_ID_MAX_LEN + 1);
        assert_eq!(
            ConversationId::parse(&raw),
            Err(UnsafeConversationId(raw.clone()))
        );
    }

    #[test]
    fn an_id_at_the_length_limit_is_accepted() {
        let raw = "a".repeat(CONVERSATION_ID_MAX_LEN);
        assert_eq!(
            ConversationId::parse(&raw).map(|id| id.as_str().len()),
            Ok(CONVERSATION_ID_MAX_LEN)
        );
    }

    #[test]
    fn an_id_with_a_space_is_refused() {
        assert_eq!(
            ConversationId::parse("my conv"),
            Err(UnsafeConversationId("my conv".into()))
        );
    }
}
