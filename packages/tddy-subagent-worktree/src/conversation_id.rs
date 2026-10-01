//! The conversation id, checked before it becomes a directory name and a branch name.
//!
//! The caller chooses a conversation's id (`subagent_new_session { sessionId }`), and this crate
//! puts it in two places where an arbitrary string is dangerous: a path under the session worktree
//! and a ref name in the repository's common dir. So it is refused unless it is a plain component.

use std::fmt;

/// The longest id accepted — long enough for a UUID with a prefix, short enough for a path.
pub const CONVERSATION_ID_MAX_LEN: usize = 64;

/// A conversation id that is safe as one path component and one ref component:
/// `[A-Za-z0-9._-]{1,64}`, and a valid component of a git ref name (`git check-ref-format`): not
/// starting with `.`, not containing `..`, not ending with `.` or `.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConversationId(String);

/// Why an id was refused, carrying the id as given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsafeConversationId(pub String);

impl ConversationId {
    /// Accept `raw` as a conversation id, or say why not.
    pub fn parse(raw: &str) -> Result<Self, UnsafeConversationId> {
        let plain = !raw.is_empty()
            && raw.len() <= CONVERSATION_ID_MAX_LEN
            && !raw.starts_with('.')
            && !raw.contains("..")
            && !raw.ends_with('.')
            && !raw.ends_with(".lock")
            && raw
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if plain {
            Ok(Self(raw.to_string()))
        } else {
            Err(UnsafeConversationId(raw.to_string()))
        }
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
             characters of [A-Za-z0-9._-], and a valid git ref component: it must not start or end \
             with '.', contain '..', or end with '.lock'",
            self.0
        )
    }
}

impl std::error::Error for UnsafeConversationId {}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::uuid_like("conv-3f9c2ab0-1d2e.v2_final")]
    #[case::at_the_length_limit(&"a".repeat(CONVERSATION_ID_MAX_LEN))]
    #[case::a_single_dot_inside("a.b")]
    #[case::lock_in_the_middle("a.lock.b")]
    fn a_plain_id_is_accepted(#[case] raw: &str) {
        assert_eq!(
            ConversationId::parse(raw).map(|id| id.as_str().to_string()),
            Ok(raw.to_string())
        );
    }

    #[rstest]
    #[case::slash("a/b")]
    #[case::parent_directory("..")]
    #[case::leading_dot(".hidden")]
    #[case::empty("")]
    #[case::one_past_the_length_limit(&"a".repeat(CONVERSATION_ID_MAX_LEN + 1))]
    #[case::space("my conv")]
    #[case::double_dot_inside("a..b")]
    #[case::trailing_dot("conv.")]
    #[case::lock_suffix("conv.lock")]
    #[case::lock_only(".lock")]
    fn an_id_that_cannot_be_a_directory_and_a_ref_component_is_refused(#[case] raw: &str) {
        assert_eq!(
            ConversationId::parse(raw),
            Err(UnsafeConversationId(raw.to_string()))
        );
    }
}
