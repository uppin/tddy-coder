use tddy_service::proto::connection::ProjectEntry as ProtoProjectEntry;
use crate::{connection_service::stack_parent, project_storage::{self}};

use crate::config::DaemonConfig;

use std::path::Path;

/// Write `.claude/settings.local.json` into `cwd` — the directory `claude` will run in — so Claude
/// Code wires this session's lifecycle hooks on startup.
///
/// Warn-and-continue: a session without hooks reports no status, which is worse than a session that
/// never started only if the operator cannot see it at all, and it still can.
pub(crate) fn write_claude_hooks_settings(cwd: &Path, params: &tddy_core::HookCommandParams<'_>) {
    let settings = tddy_core::build_claude_hooks_settings(params);
    let claude_dir = cwd.join(".claude");
    if let Err(e) = std::fs::create_dir_all(&claude_dir).and_then(|_| {
        serde_json::to_string_pretty(&settings)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|json| {
                tddy_core::atomic_file::write_atomic(&claude_dir.join("settings.local.json"), json)
            })
    }) {
        log::warn!(
            "session {}: failed to write .claude/settings.local.json — hooks will not fire: {e}",
            params.session_id
        );
    }
}

/// The web port a hook URL assumes when `listen.web_port` is unset. `startup` refuses to serve
/// without that setting, so this only covers a config the daemon would not have started from — but
/// building the URL is not the place to discover it.
const DEFAULT_WEB_PORT: u16 = 8899;

/// Where a hook command reaches this daemon when nothing is configured: its own web listener on
/// loopback.
///
/// The port default is here and nowhere else — a hook posting to the wrong port fails silently from
/// the operator's side, and three copies of `8899` is three chances for one of them to fall behind a
/// changed default.
pub fn local_daemon_hook_url(config: &DaemonConfig) -> String {
    format!(
        "http://127.0.0.1:{}",
        config.listen.web_port.unwrap_or(DEFAULT_WEB_PORT)
    )
}

/// Externally-reachable HTTP base URL peer daemons use to reach this daemon's Connect-HTTP surface
/// (today: `auth.LiveKitTokenService/MintLiveKitToken`, used by `tddy-remote-git-repo` to mint the
/// common-room LiveKit token before driving `remote_git.RemoteGitService/Serve`).
///
/// Explicit `listen.advertise_url` wins; otherwise the loopback URL derived from the web port —
/// the same default `claude_hook_daemon_url` falls back to, and for the same reason: a daemon that
/// never configured an external URL is one a peer on another host cannot reach, but one a peer on
/// the same host (and every test) can. The facilitating daemon publishes this in
/// `AgentClonePlacement.facilitating_daemon_url` so an owning daemon that has never seen the
/// project can clone it (PRD AC37).
pub fn advertise_daemon_url(config: &DaemonConfig) -> String {
    config
        .listen
        .advertise_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// Base URL a claude-cli session's hook commands call `ReportSessionStatus` on: the configured
/// `claude_cli.daemon_url`, else this daemon's own web port.
pub fn claude_hook_daemon_url(config: &DaemonConfig) -> String {
    config
        .claude_cli
        .as_ref()
        .and_then(|c| c.daemon_url.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| local_daemon_hook_url(config))
}

/// Resolve the `claude` binary for the interactive (non-sandboxed) StartSession path.
///
/// Delegates to [`crate::config::resolve_claude_binary_path`] so the interactive and sandboxed
/// spawn paths never diverge on which `claude` they pick (explicit config path honored; bare name
/// auto-resolved to a real host install).
pub fn resolve_start_session_claude_binary(config: &DaemonConfig) -> String {
    crate::config::resolve_claude_binary_path(config)
}

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

/// Resolves the default remote name for a registered project, degrading to an empty string when the
/// resolver itself errors (e.g. unreadable `projects.yaml`) so a list RPC never fails on a single
/// bad row. The resolver already falls back to `origin` as the last resort, so the empty case is the
/// rare "registry unreadable" path — clients apply their own `origin` fallback then.
pub(crate) fn resolve_default_remote_or_empty(
    projects_dir: &Path,
    project_id: &str,
    repo_root: &Path,
) -> String {
    project_storage::effective_remote_name_for_project(projects_dir, project_id, repo_root)
        .unwrap_or_default()
}

/// Builds a proto [`ProjectEntry`] from a stored [`project_storage::ProjectData`] plus the resolved
/// `default_remote`. Centralizing the mapping keeps every response (ListProjects, CreateProject,
/// AddProjectToHost, SetProjectDefaultBranch) consistent as fields are added.
pub(crate) fn project_entry_from(
    p: &project_storage::ProjectData,
    daemon_instance_id: String,
    default_remote: String,
) -> ProtoProjectEntry {
    ProtoProjectEntry {
        project_id: p.project_id.clone(),
        name: p.name.clone(),
        git_url: p.git_url.clone(),
        main_repo_path: p.main_repo_path.clone(),
        daemon_instance_id,
        main_branch_ref: p.main_branch_ref.clone().unwrap_or_default(),
        default_remote,
    }
}

/// The repoint target a client may act on: `Ok(None)` for "no target named", `Ok(Some(target))`
/// for an accepted one, `Err(reason)` for a target the daemon refuses.
///
/// `RepointPlannedPrRequest.target_base_branch` is applied by `repoint_planned_pr_node` as a
/// **retain** rule — the parents that own that branch stay and the rest are dropped — so a target
/// no parent owns *is* the instruction to detach the node onto the default branch. Validation is
/// therefore not politeness: a stale label, a typo, or a client that has drifted from the daemon's
/// view of the repo would each read as "detach this node" and silently rewrite the plan. An
/// accepted target must name either the resolved default branch or one of the node's parents'
/// branches; nothing else is a meaningful thing to be based onto.
///
/// An empty or whitespace-only target is not a rejection: it names no target at all and selects the
/// original drop-merged-parents rule (`None`).
///
/// The default branch is compared with the remote prefix stripped from both sides.
/// `tddy_core::resolve_default_integration_base_ref` returns a remote-tracking ref
/// (`<remote>/<branch>`), while a node's `branch` and a GitHub PR base are plain names, so the label
/// a client renders can legitimately carry either form. The remote is parsed off `default_branch`
/// (the segment before its first `/`) so a non-`origin` default is normalized correctly. The
/// accepted value returned is the caller's own trimmed input, not the normalized form, so the
/// recipe matches parent branches as recorded.
pub fn validate_repoint_target(
    target_base_branch: &str,
    default_branch: &str,
    parent_branches: &[&str],
) -> Result<Option<String>, String> {
    let target = target_base_branch.trim();
    if target.is_empty() {
        return Ok(None);
    }

    let remote = default_branch
        .split_once('/')
        .map(|(r, _)| r)
        .unwrap_or("origin");
    let names_default = tddy_core::worktree::local_branch_name_for_remote(target, remote)
        == tddy_core::worktree::local_branch_name_for_remote(default_branch, remote);
    let names_parent = parent_branches.contains(&target);

    if names_default || names_parent {
        Ok(Some(target.to_string()))
    } else {
        Err(format!(
            "target_base_branch '{target}' names neither the default branch '{default_branch}' nor any parent's branch"
        ))
    }
}

/// Resolve the `claude` binary for a ResumeSession relaunch through the same host resolver as
/// StartSession, so an explicitly configured path is honored and a bare name is resolved to a host
/// path instead of being spawned against the daemon's minimal systemd PATH.
pub fn resolve_resume_session_claude_binary(config: &DaemonConfig) -> String {
    crate::config::resolve_claude_binary_path(config)
}
