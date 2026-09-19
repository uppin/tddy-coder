//! The git environment a session's own commits are made under.
//!
//! A session's checkout is a worktree of somebody's repository, and `git` in it would otherwise
//! author commits as whatever `user.name` that checkout inherited — the machine's global config,
//! or nothing at all. Neither is the answer this daemon wants: the commits belong to the GitHub
//! account the **project** assigns (`#keyring` 5/9), and that account is the same one whose token
//! authenticates the push.
//!
//! So the environment is built from an [`ActingIdentity`] and from nothing else. There is no
//! variant of this function that reads the process environment, and none that takes a name and
//! email separately — the single argument is what keeps the identity and the token that pushes it
//! from drifting apart, because there is only one value they can both come from.
//!
//! # Not the snapshot identity
//!
//! `session_room::publish_wip_ref` signs its work-in-progress commits as `tddy-daemon` and goes on
//! doing so. Those objects are a *measurement* of the agent's working tree, not the agent's work,
//! and attributing them to a person's GitHub account would put commits in a person's name that
//! the person never made. The two identities are separate on purpose; this module owns one of them.

use tddy_accounts::ActingIdentity;

/// The four `git` environment variables a session's commits are made under.
///
/// Author and committer are the same account: a session's commit has one maker. Returned as pairs
/// rather than applied to a `Command` here, because the caller that spawns the agent is the only
/// thing that knows which process they belong on — and because a list of pairs is something a test
/// can read without running `git`.
#[must_use]
pub fn session_git_environment(acting: &ActingIdentity) -> Vec<(String, String)> {
    let _ = acting;
    todo!("TODO(keyring 9/9): author and committer, both the assigned account, and nothing else")
}
