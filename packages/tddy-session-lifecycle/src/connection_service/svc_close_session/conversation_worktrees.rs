use crate::connection_service::svc_close_session::close_plan::PlannedWorktree;
use crate::connection_service::svc_close_session::close_plan::Removal;
use crate::connection_service::svc_close_session::worktree_reads::read_local_worktree;
use crate::connection_service::svc_close_session::worktree_reads::unavailable;
use std::path::Path;
use tddy_service::proto::session::{CloseSessionWorktree, CloseSessionWorktreeOwner};
use tddy_subagent_worktree::{ConversationId, ConversationWorktrees, SUBAGENT_WORKTREES_DIR};

/// Every clone-placed conversation worktree under the session worktree `session_worktree`, each
/// read for what removing it would lose (above the commit the conversation started from).
pub(super) async fn conversation_worktrees_in(
    session_worktree: &Path,
    session_id: &str,
) -> Vec<PlannedWorktree> {
    let Ok(entries) = std::fs::read_dir(session_worktree.join(SUBAGENT_WORKTREES_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    let worktrees = ConversationWorktrees::new(session_worktree, session_id);
    let mut planned = Vec::new();
    for name in names {
        let root = session_worktree.join(SUBAGENT_WORKTREES_DIR).join(&name);
        let without_base = || Removal::ConversationWithoutBase {
            session_worktree: session_worktree.to_path_buf(),
            root: root.clone(),
        };
        // A directory that names no conversation, or whose conversation recorded no base, is still
        // read — removing the session's worktree would take whatever it holds along.
        let found = match ConversationId::parse(&name) {
            Ok(conversation) => worktrees.existing(&conversation).await,
            Err(_) => Ok(None),
        };
        let (read, removal) = match found {
            Ok(Some(found)) => (
                CloseSessionWorktree {
                    branch: found.branch().to_string(),
                    ..read_local_worktree(found.root(), Some(found.base())).await
                },
                Removal::Conversation(found),
            ),
            // Not a checkout of its own: git would read the enclosing session worktree instead,
            // which excludes this directory, so what it holds is never reported as safe to lose.
            Ok(None) if !root.join(".git").exists() => (
                unavailable(
                    CloseSessionWorktree::default(),
                    format!(
                        "{} is not a git worktree, so what it holds cannot be read; removing the \
                         session's worktree would delete it",
                        root.display()
                    ),
                ),
                without_base(),
            ),
            Ok(None) => (read_local_worktree(&root, None).await, without_base()),
            Err(error) => (
                unavailable(CloseSessionWorktree::default(), error.to_string()),
                without_base(),
            ),
        };
        planned.push(PlannedWorktree {
            view: CloseSessionWorktree {
                path: root.to_string_lossy().into_owned(),
                owner: CloseSessionWorktreeOwner::Conversation as i32,
                owner_id: name,
                ..read
            },
            removal,
        });
    }
    planned
}
