use crate::connection_service::svc_close_session::close_plan::PlannedWorktree;
use crate::connection_service::svc_close_session::close_plan::Removal;
use crate::connection_service::svc_close_session::worktree_reads::read_local_worktree;
use crate::connection_service::svc_close_session::worktree_reads::unavailable;
use std::path::Path;
use tddy_service::proto::session::{CloseSessionWorktree, CloseSessionWorktreeOwner};
use tddy_subagent_worktree::{ConversationId, ConversationWorktrees, SUBAGENT_WORKTREES_DIR};

/// Every clone-placed conversation worktree under the session worktree `session_worktree`, each
/// read for what removing it would lose (above the commit the conversation started from). A
/// directory of them that cannot be listed is one worktree that could not be read: removing the
/// session's worktree would delete whatever it holds.
pub(super) async fn conversation_worktrees_in(
    session_worktree: &Path,
    session_id: &str,
) -> Vec<PlannedWorktree> {
    let directory = session_worktree.join(SUBAGENT_WORKTREES_DIR);
    let names = match conversation_directory_names(&directory) {
        Ok(names) => names,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![unlistable_conversations(&directory, &error)],
    };
    let worktrees = ConversationWorktrees::new(session_worktree, session_id);
    let mut planned = Vec::new();
    for name in names {
        plan_conversation_worktree(session_worktree, &worktrees, &mut planned, name).await;
    }
    planned
}

/// Plan the conversation worktree in directory `name` of the session worktree's conversations, read
/// for what removing it would lose — or why it cannot be — and add it to `planned`.
async fn plan_conversation_worktree(
    session_worktree: &Path,
    worktrees: &ConversationWorktrees,
    planned: &mut Vec<PlannedWorktree>,
    name: String,
) {
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

/// The name of every directory in `directory`, sorted. Only "there is no such directory" is no
/// conversation; any other failure to read it, or one of its entries, is the error.
fn conversation_directory_names(directory: &Path) -> std::io::Result<Vec<String>> {
    conversation_directory_names_on(&tddy_fs::DiskFs, directory)
}

/// [`conversation_directory_names`] through the filesystem port.
pub fn conversation_directory_names_on(
    fs: &dyn tddy_fs::Fs,
    directory: &Path,
) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for path in fs.read_dir(directory)? {
        // `metadata_follow`, as before: the walk follows a link, and a link to nothing holds
        // nothing.
        match fs.metadata_follow(&path) {
            Ok(meta) if meta.kind == tddy_fs::FsKind::Dir => {}
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} is not named in UTF-8", path.display()),
                )
            })?;
        names.push(name.to_string());
    }
    names.sort();
    Ok(names)
}

/// The conversation worktrees' `directory`, which could not be listed: one worktree that could not
/// be read, which blocks the close. Removed, were the close ever to run, with the session's
/// worktree it sits in.
fn unlistable_conversations(directory: &Path, error: &std::io::Error) -> PlannedWorktree {
    PlannedWorktree {
        view: CloseSessionWorktree {
            path: directory.to_string_lossy().into_owned(),
            owner: CloseSessionWorktreeOwner::Conversation as i32,
            ..unavailable(
                CloseSessionWorktree::default(),
                format!(
                    "{} cannot be read, so the conversation worktrees in it cannot be listed; \
                     removing the session's worktree would delete them: {error}",
                    directory.display()
                ),
            )
        },
        removal: Removal::WithAnother,
    }
}
