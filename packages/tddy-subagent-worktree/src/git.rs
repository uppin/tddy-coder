//! The one way this crate runs git.
//!
//! Null stdin, `GIT_OPTIONAL_LOCKS=0` (a status-like read must not take the index lock a caller's
//! own `git` may be holding), a fixed committer identity (the repository's own `user.*` may be
//! unset, and a subagent's commit must not be attributed to the developer), and the command line
//! carried in the error so a failure names what was run.

use std::ffi::OsStr;
use std::path::Path;

use crate::worktree::WorktreeError;

// TODO(isolated-edits): drop the allow once the worktree operations call the runner
#[allow(dead_code)]
pub(crate) const COMMITTER_NAME: &str = "tddy-subagent";
#[allow(dead_code)]
pub(crate) const COMMITTER_EMAIL: &str = "tddy-subagent@tddy.invalid";

#[allow(dead_code)]
/// Run `git <args>` in `dir`, optionally feeding `stdin`, and return its stdout.
pub(crate) async fn git<I, S>(
    dir: &Path,
    args: I,
    env: &[(&str, &OsStr)],
    stdin: Option<&[u8]>,
) -> Result<String, WorktreeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // TODO(isolated-edits): implement
    let _ = (dir, env, stdin);
    let _args: Vec<S> = args.into_iter().collect();
    todo!("git runner")
}
