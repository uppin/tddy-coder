use crate::connection_service::stack_parent;

use tddy_daemon_kernel::config::DaemonConfig;

use std::path::Path;

// TODO(carve-20-daemon-urls): empty since its helpers moved to `daemon_hook_urls`; delete this module and
// `hooks_and_urls/daemon_urls.rs` (a file deletion, which needs the developer's consent).
pub(crate) mod daemon_urls;

/// The branch a spawn actually operates on: the branch it creates, or — under
/// `work_on_selected_branch` — the existing branch it resumes.
///
/// A PR-stack node's link is keyed on this rather than on `new_branch_name`, which is **empty** for a
/// resume. Recovering a planned PR whose child session was deleted means resuming the branch the node
/// already owns (it exists, is pushed, has a worktree), and without the effective branch
/// `pr_stack_node_for_spawn` matches nothing: the node would never re-link, so the row would stay
/// recovered and every click would spawn another unlinked session.
///
/// A blank intent defaults to `new_branch_from_base` (`StartSessionRequest.branch_worktree_intent`),
/// and a resume ignores any leftover `new_branch_name` the dialog carried — keying on a branch the
/// spawn never touches would link the node to the wrong branch.
///
/// A resumed branch is reduced to its local name: the dialog's picker is fed by
/// `ListProjectBranches`, which offers remote-tracking names (`<remote>/<branch>`), while a stack
/// node records the local one. Keying on the prefixed form matches no node, which is the same
/// silent non-link this function exists to prevent. The `remote` argument is the project's resolved
/// default remote so a non-`origin` prefix is stripped correctly.
#[must_use]
pub fn effective_spawn_branch<'a>(
    branch_worktree_intent: &str,
    new_branch_name: &'a str,
    selected_branch_to_work_on: &'a str,
    remote: &str,
) -> &'a str {
    match branch_worktree_intent.trim() {
        "work_on_selected_branch" => {
            tddy_core::worktree::local_branch_name_for_remote(selected_branch_to_work_on, remote)
        }
        _ => new_branch_name.trim(),
    }
}

/// A claude-cli session as it stands the moment its LiveKit participant is created: everything the
/// daemon knows about it, and nothing it would have to ask anyone for.
///
/// One value rather than six arguments because the association half of it is meaningless piecewise
/// — a `stack_node_id` without the orchestrator that holds it names nothing — and because the whole
/// point of naming this input is that what the participant advertises can be asserted.
pub struct StartingClaudeCliSession<'a> {
    /// The session's own id. The web recovers it from the participant identity too, but a block
    /// that does not name itself cannot be checked against that.
    pub session_id: &'a str,
    /// The model the agent runs.
    pub model: &'a str,
    /// The managed workflow recipe, empty for an unmanaged session.
    pub recipe: &'a str,
    /// The checkout the session works in.
    pub worktree_path: &'a Path,
    /// The branch the session created, as its own changeset records it — see
    /// [`spawned_branch_of_session`].
    pub branch: &'a str,
    /// The pr-stack orchestrator this session was spawned under, if any.
    pub stack_parent: &'a stack_parent::SpawnStackParent<'a>,
}

/// The `session` block a claude-cli session's LiveKit participant publishes about itself.
///
/// A claude-cli session published **no** participant metadata at all, and planned-PR children are
/// claude-cli sessions: a child started on another host therefore arrived in the drawer as a
/// synthesized row carrying no `branch` and no `orchestrator_session_id` — the two keys every
/// PR-stack join uses. Presence is the only cross-host signal the web has, because `ListSessions`
/// does not fan out (D37).
///
/// Static fields only. The live workflow fields (`goal`, `state`, `activity_status`,
/// `elapsed_display`, `pending_elicitation`) stay empty for a claude-cli session, exactly as they
/// were when nothing was published: filling them needs a workflow tap the way `tddy-coder` has one,
/// which is logged in `docs/dev/TODO.md` rather than closed here.
///
/// A session that belongs to no stack publishes the association keys **empty, never absent**: the
/// merge into participant metadata is shallow, so an omitted key would erase a sibling publisher's
/// value rather than leave it alone — and empty is a fact ("this session is nobody's stack child")
/// that a reader can act on, where a missing key is indistinguishable from an older publisher.
#[must_use]
pub fn claude_cli_participant_metadata(
    session: &StartingClaudeCliSession<'_>,
) -> tddy_core::session_participant_metadata::SessionParticipantMetadata {
    tddy_core::session_participant_metadata::SessionParticipantMetadata {
        agent: "claude".to_string(),
        model: session.model.to_string(),
        recipe: session.recipe.to_string(),
        repo_path: session.worktree_path.to_string_lossy().to_string(),
        session_id: session.session_id.to_string(),
        orchestrator_session_id: session
            .stack_parent
            .session_id()
            .unwrap_or_default()
            .to_string(),
        stack_node_id: session
            .stack_parent
            .stack_node_id()
            .unwrap_or_default()
            .to_string(),
        branch: session.branch.to_string(),
        ..Default::default()
    }
}

/// The branch a spawn's child session **actually** ended up on: the one its worktree setup recorded
/// in `changeset.yaml`, falling back to `requested_branch` only when the session recorded none.
///
/// [`effective_spawn_branch`] answers which branch the *request* asked for, and that is not always
/// what exists. `create_worktree_with_retry` appends `-1`, `-2`, … when the name is already taken —
/// the **default** conflict behaviour (`on_branch_conflict = ""`), and one that also fires for a
/// collision with a branch no session owns, which the `reject` guard does not cover — and writes the
/// suffixed name into the session's changeset.
///
/// Recording the requested name on a planned node would advertise a branch nobody has: the node's
/// descendants base onto `<remote>/<requested>`, which does not exist, and the cross-host row
/// synthesized from the child's participant metadata names a branch no host can resolve.
/// [`push_new_branch_to_origin_if_requested`] already reads the branch back for exactly this reason.
///
/// The fallback is not a guess: a spawn against a client-supplied `repo_path` creates no worktree
/// and writes no branch, and there the requested name is the only answer there is. An unreadable
/// changeset is logged rather than swallowed — the branch it holds is what the whole link is keyed
/// on, so losing it silently is the failure this function exists to prevent.
#[must_use]
pub fn spawned_branch_of_session(session_dir: &Path, requested_branch: &str) -> String {
    match tddy_core::read_changeset(session_dir) {
        Ok(cs) => cs
            .branch
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| requested_branch.trim().to_string()),
        Err(e) => {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "could not read the changeset at {} to learn the branch the spawn created ({e}); keying the pr-stack link on the requested name '{}' instead, which is wrong if the branch was suffixed on a name collision",
                session_dir.display(),
                requested_branch.trim()
            );
            requested_branch.trim().to_string()
        }
    }
}

/// Resolve the `claude` binary for a ResumeSession relaunch through the same host resolver as
/// StartSession, so an explicitly configured path is honored and a bare name is resolved to a host
/// path instead of being spawned against the daemon's minimal systemd PATH.
pub fn resolve_resume_session_claude_binary(config: &DaemonConfig) -> String {
    tddy_daemon_kernel::config::resolve_claude_binary_path(config)
}
