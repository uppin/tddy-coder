//! Where inside a jail a subagent conversation's tool call runs.
//!
//! The daemon creates and commits in a conversation's worktree — git runs on the host, which can
//! see the repository's common dir — and the jail mounts the session worktree that contains it.
//! The jail therefore only *finds* the root; it never runs git, and this crate does not depend on
//! the crate that does.

use std::path::{Component, Path, PathBuf};

/// Where conversation worktrees live under the session worktree.
///
/// The same directory `tddy_subagent_worktree::SUBAGENT_WORKTREES_DIR` names; the runner cannot
/// depend on that crate (it would put the git mechanics inside every jail), so
/// `tddy-daemon-sandbox`, which depends on both, pins the two equal in a test.
pub const CONVERSATION_WORKTREES_DIR: &str = "tmp/subagent-worktrees";

/// `<worktree>/tmp/subagent-worktrees/<conversation_id>`, or the worktree itself for an empty id.
///
/// An id that is not exactly one plain path segment is an error: the daemon already refused it, so
/// one arriving here is a request that did not come through the daemon's checks.
pub(crate) fn tool_root(worktree: &Path, conversation_id: &str) -> Result<PathBuf, String> {
    if conversation_id.is_empty() {
        return Ok(worktree.to_path_buf());
    }
    let mut segments = Path::new(conversation_id).components();
    match (segments.next(), segments.next()) {
        (Some(Component::Normal(segment)), None) if segment == conversation_id => {
            Ok(worktree.join(CONVERSATION_WORKTREES_DIR).join(segment))
        }
        _ => Err(format!(
            "conversation id {conversation_id:?} is not a single path segment"
        )),
    }
}

/// The bytes to relay for `service`/`method`: a conversation-worktree request is rewritten to name
/// the session this jail serves and to carry no credential, anything else passes through.
///
/// The host answers that operation with no token to check — the jail is trusted for exactly the one
/// session it was built for, as it is for every tool call — so the session it names must be the
/// runner's own, never whatever the agent inside put in the request.
pub(crate) fn bind_to_this_session(
    service: &str,
    method: &str,
    payload: &[u8],
    session_id: &str,
) -> Result<Vec<u8>, tddy_rpc::Status> {
    use prost::Message;
    use tddy_service::proto::exec_tools::ConversationWorktreeRequest;

    if !tddy_tool_engine::IN_JAIL_RELAYABLE_EXEC_TOOLS.contains(&(service, method)) {
        return Ok(payload.to_vec());
    }
    let request = ConversationWorktreeRequest::decode(payload).map_err(|e| {
        tddy_rpc::Status::invalid_argument(format!("decode ConversationWorktreeRequest: {e}"))
    })?;
    Ok(ConversationWorktreeRequest {
        session_id: session_id.to_string(),
        session_token: String::new(),
        daemon_instance_id: String::new(),
        ..request
    }
    .encode_to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_worktree_request_is_rebound_to_the_jails_own_session() {
        use prost::Message;
        use tddy_service::proto::exec_tools::ConversationWorktreeRequest;
        let claimed = ConversationWorktreeRequest {
            session_id: "someone-elses".into(),
            session_token: "forged".into(),
            conversation_id: "explore".into(),
            ..Default::default()
        };

        let relayed = bind_to_this_session(
            "exec_tools.ExecToolService",
            "ConversationWorktree",
            &claimed.encode_to_vec(),
            "mine",
        )
        .unwrap();

        let relayed = ConversationWorktreeRequest::decode(relayed.as_slice()).unwrap();
        assert_eq!(
            (
                relayed.session_id,
                relayed.session_token,
                relayed.conversation_id
            ),
            ("mine".to_string(), String::new(), "explore".to_string())
        );
    }

    #[test]
    fn an_empty_conversation_runs_at_the_worktree() {
        assert_eq!(tool_root(Path::new("/w"), "").unwrap(), PathBuf::from("/w"));
    }

    #[test]
    fn a_conversation_runs_in_its_own_directory_under_the_worktree() {
        assert_eq!(
            tool_root(Path::new("/w"), "explore").unwrap(),
            PathBuf::from("/w/tmp/subagent-worktrees/explore")
        );
    }

    #[test]
    fn an_id_that_leaves_its_segment_is_refused() {
        assert_eq!(
            ["../escape", "a/b", "..", "/abs"].map(|id| tool_root(Path::new("/w"), id).is_err()),
            [true; 4]
        );
    }
}
