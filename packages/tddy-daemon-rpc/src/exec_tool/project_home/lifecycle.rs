//! `SetSessionMark`, `DeleteSession`, `ClearWorktree`, `DeleteWorktree` and `RestoreWorktree`
//! answered on this host (`#orchestrators` 5/11): the mark is written to the target's
//! `.session.yaml`, a delete is the web's Delete, and the three worktree actions call the
//! `tddy-worktree-service` functions the Worktrees screen's RPCs call.

use crate::exec_tool::project_home::DaemonProjectHome;
use tddy_tool_engine::project_home_lifecycle_tools::ProjectHomeLifecyclePort;

// TODO(session-lifecycle-tools): implement — `set_session_mark` (write `orchestration_mark`),
// `delete_session` (`delete_session_as`), `clear_worktree` (`clean_worktree_under_repo`),
// `delete_worktree` (`remove_worktree_under_repo`, refusing uncommitted changes naming the commit
// and stash tools), `project_home_worktree` (`home_worktree_path` under the project's worktrees
// root) and `restore_worktree` (the `RestoreSessionWorktree` path). Until then the trait's
// defaults refuse.
impl ProjectHomeLifecyclePort for DaemonProjectHome {}
