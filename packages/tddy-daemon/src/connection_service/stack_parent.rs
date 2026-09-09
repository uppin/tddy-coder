use super::ConnectionServiceImpl;

use tddy_rpc::Status;

use std::path::Path;

/// A spawn's PR-stack parent, as the daemon that resolves it needs to see it.
///
/// Carried as one value because the resolution needs facts from both sides of the routing decision
/// and none can be derived from the others: `sessions_base` and `repo_root` are *this* daemon's,
/// read when the parent turns out to be this daemon's own, while `session_token` and `project_id`
/// are what a peer needs — the credential it verifies the question with, and the logical project id
/// (stable across hosts, because `AddProjectToHost` reuses it) it resolves its own checkout from.
pub struct StackBaseLookup<'a> {
    /// Authenticates the question wherever it is answered. A peer verifies the same stateless token
    /// with the `livekit.api_secret` both daemons share, so no second credential is minted.
    pub session_token: &'a str,
    /// The parent session id. `None` — or blank — is a spawn with no stack parent at all, which
    /// resolves to no chain base without asking anyone.
    pub stack_parent: Option<&'a str>,
    /// The daemon whose sessions tree holds the parent. Empty = this one.
    pub stack_parent_daemon_instance_id: &'a str,
    /// The logical project the child is being started under.
    pub project_id: &'a str,
    /// This daemon's sessions tree.
    pub sessions_base: &'a Path,
    /// This daemon's checkout of `project_id`.
    pub repo_root: &'a Path,
    /// The branch the child spawn is about to create — how the planned node it belongs to is found
    /// when, and only when, `stack_node_id` is empty.
    pub new_branch_name: &'a str,
    /// The planned node the spawn materializes, as the surface that started it named it. Preferred
    /// over the branch (D34) — see [`tddy_core::pr_stack_node_for_spawn`]: a renamed branch matches
    /// no node, and a node matched by nothing means no ordering gate runs at all.
    pub stack_node_id: &'a str,
    /// The operator-chosen base from the Start-session dialog's "Base branch" selector. When
    /// non-empty after trim, chain-base resolution short-circuits before the stack ordering gate
    /// — that deliberate repoint is honored via [`tddy_core::select_worktree_base_ref`].
    pub selected_integration_base_ref: &'a str,
}

/// A spawn's link back onto the planned node it materializes, as the daemon that records it needs
/// to see it.
///
/// Carried as one value for the same reason [`StackBaseLookup`] is: `orchestrator_session_id` alone
/// says nothing about *which disk* holds the plan it names, and recording the link on the wrong one
/// is exactly the bug this type exists to close. `sessions_base` is this daemon's own tree, read
/// only on the branch-derived local path — a spawn that names no node, which is the orchestrator
/// agent's own `spawn-child` and always runs on the orchestrator's host.
pub struct StackNodeLink<'a> {
    /// Authenticates the write wherever it is performed. A peer verifies the same stateless token
    /// with the `livekit.api_secret` both daemons share.
    pub session_token: &'a str,
    /// The pr-stack orchestrator whose plan holds the node.
    pub orchestrator_session_id: &'a str,
    /// The daemon whose sessions tree holds that orchestrator. Empty = this one.
    pub orchestrator_daemon_instance_id: &'a str,
    /// The planned node the spawn materializes. Empty = the caller named none, and the node is
    /// looked up locally by the branch instead (D34).
    pub node_id: &'a str,
    /// The session that materialized the node.
    pub child_session_id: &'a str,
    /// The branch that session created — the load-bearing half, since a node owning no branch
    /// refuses every descendant.
    pub branch: &'a str,
    /// This daemon's sessions tree, for the branch-derived local write.
    pub sessions_base: &'a Path,
}

/// This daemon in its capacity as the resolver of a spawn's PR-stack parent.
///
/// A trait for the same reason [`SeededAgentClones`] is one: the co-located claude-cli and
/// cursor-cli spawns are free functions, and reaching the daemon that owns a parent is the whole of
/// `ConnectionService`'s peer-facing surface — naming that type there would drag it through every
/// caller of a function that otherwise mentions nothing of the kind.
#[async_trait::async_trait]
pub trait StackParentHost: Send + Sync {
    /// The ref the child's worktree is cut from, or `None` when the parent names no chain base and
    /// the project default applies.
    async fn chain_base_ref(&self, lookup: &StackBaseLookup<'_>) -> Result<Option<String>, Status>;

    /// Record the spawn's session and branch on the planned node it materializes, wherever that
    /// node's orchestrator lives.
    async fn link_spawned_branch(&self, link: &StackNodeLink<'_>) -> Result<(), Status>;
}

/// A spawn's PR-stack parent as the co-located spawn paths carry it: which session, whose sessions
/// tree holds it, and the daemon that answers what the child's worktree bases off.
///
/// One value rather than four parameters because the four are meaningless apart — a parent session
/// id without the host that owns it is exactly the bug this type exists to close — and an enum
/// because a spawn with no stack parent names no host either. Nothing resolves it, so there is
/// nothing for a caller to supply.
pub enum SpawnStackParent<'a> {
    /// The spawn is not part of a stack: no parent, and therefore no chain base. The child's
    /// worktree is cut from the project default.
    NoParent,
    /// The spawn descends from `session_id`, held by `daemon_instance_id` (empty = this daemon).
    OwnedBy {
        /// The parent session id.
        session_id: &'a str,
        /// The daemon whose sessions tree holds it. Empty = this one.
        daemon_instance_id: &'a str,
        /// The planned node of that parent's stack this spawn materializes, as the surface that
        /// started it named it. Empty when the caller named none — the orchestrator agent's own
        /// `spawn-child`, which runs on the orchestrator's host where the node can be found from
        /// the branch instead (D34).
        stack_node_id: &'a str,
        /// The credential a peer verifies the forwarded question with. Empty when the parent is
        /// this daemon's own session — an agent-driven spawn is made by the orchestrator's own
        /// process, has no caller token, and asks nothing of any peer.
        session_token: &'a str,
        /// The daemon that resolves the parent: this one, forwarding to the owner when it is not.
        host: &'a dyn StackParentHost,
    },
}

impl<'a> SpawnStackParent<'a> {
    /// The parent this spawn records as its orchestrator, if any.
    pub fn session_id(&self) -> Option<&'a str> {
        match self {
            Self::NoParent => None,
            Self::OwnedBy { session_id, .. } => Some(session_id),
        }
    }

    /// The daemon whose sessions tree holds the parent, for logging what a failed link addressed.
    pub fn daemon_instance_id(&self) -> Option<&'a str> {
        match self {
            Self::NoParent => None,
            Self::OwnedBy {
                daemon_instance_id, ..
            } => Some(daemon_instance_id),
        }
    }

    /// The planned node this spawn materializes, as the caller named it.
    pub fn stack_node_id(&self) -> Option<&'a str> {
        match self {
            Self::NoParent => None,
            Self::OwnedBy { stack_node_id, .. } => Some(stack_node_id),
        }
    }

    /// What the child's worktree is cut from, resolved by whichever daemon owns the parent, or
    /// `None` when there is no parent — or the parent named no base for this branch, which leaves
    /// the project default to apply.
    pub async fn chain_base_ref(
        &self,
        project_id: &str,
        sessions_base: &Path,
        repo_root: &Path,
        new_branch_name: &str,
        selected_integration_base_ref: &str,
    ) -> Result<Option<String>, Status> {
        let Self::OwnedBy {
            session_id,
            daemon_instance_id,
            stack_node_id,
            session_token,
            host,
        } = self
        else {
            return Ok(None);
        };
        host.chain_base_ref(&StackBaseLookup {
            session_token,
            stack_parent: Some(session_id),
            stack_parent_daemon_instance_id: daemon_instance_id,
            project_id,
            sessions_base,
            repo_root,
            new_branch_name,
            stack_node_id,
            selected_integration_base_ref,
        })
        .await
    }

    /// Record this spawn's session and branch on the planned node it materializes, on whichever
    /// daemon owns the orchestrator. `Ok(())` for a spawn that is part of no stack.
    ///
    /// Called once the child's branch exists — that is precisely the condition
    /// [`tddy_core::changeset::Stack::base_ref_for_spawn`] gates descendants on.
    pub async fn link_spawned_branch(
        &self,
        sessions_base: &Path,
        branch: &str,
        child_session_id: &str,
    ) -> Result<(), Status> {
        let Self::OwnedBy {
            session_id,
            daemon_instance_id,
            stack_node_id,
            session_token,
            host,
        } = self
        else {
            return Ok(());
        };
        host.link_spawned_branch(&StackNodeLink {
            session_token,
            orchestrator_session_id: session_id,
            orchestrator_daemon_instance_id: daemon_instance_id,
            node_id: stack_node_id,
            child_session_id,
            branch,
            sessions_base,
        })
        .await
    }

    /// [`Self::link_spawned_branch`], with the refusal logged instead of raised (D36).
    ///
    /// The link lands *after* the worktree, the branch and the session already exist, so failing the
    /// spawn here would leave an orphan session on this host and still no branch on the
    /// orchestrator's — strictly worse than a node the operator can re-link by restarting it. The
    /// live association still travels in participant metadata (D37), and the daemon's own spawn gate
    /// is unaffected: a descendant of an unlinked node is refused for the real reason.
    ///
    /// One named seam rather than an `if let Err` at each spawn path, because "a failed link is
    /// survivable" is a decision, not an incident — and a decision no spawn path is driveable enough
    /// to assert on where it is written inline.
    pub async fn link_spawned_branch_without_failing_the_spawn(
        &self,
        sessions_base: &Path,
        branch: &str,
        child_session_id: &str,
    ) {
        if let Err(status) = self
            .link_spawned_branch(sessions_base, branch, child_session_id)
            .await
        {
            log::error!(
                target: "tddy_daemon::connection_service",
                "session {child_session_id}: could not record its branch '{branch}' on pr-stack orchestrator {:?} (node {:?}, daemon {:?}): {}; the node keeps no branch and its descendants stay unspawnable until it is re-linked",
                self.session_id().unwrap_or_default(),
                self.stack_node_id().unwrap_or_default(),
                self.daemon_instance_id().unwrap_or_default(),
                status.message()
            );
        }
    }
}

#[async_trait::async_trait]
impl StackParentHost for ConnectionServiceImpl {
    async fn chain_base_ref(&self, lookup: &StackBaseLookup<'_>) -> Result<Option<String>, Status> {
        self.resolve_chain_base_ref_status(lookup).await
    }

    async fn link_spawned_branch(&self, link: &StackNodeLink<'_>) -> Result<(), Status> {
        self.record_spawn_on_stack_node(link).await
    }
}
