//! Seeding a new orchestrator's stack with a base session the operator already has open.

use std::path::Path;

use tddy_core::changeset::StackNode;

use super::{append_node_atomic, next_free_node_id, read_stack};

/// What a base session contributes to the root node of a stack seeded on it, once every rule that
/// governs whether it may seed one has been checked.
///
/// Answered by [`check_stack_seed_base`] so that a caller which only *validates* (the daemon, before
/// it spawns anything) and the caller that *writes* (the seeding function) derive the same facts from
/// the same rules, instead of each re-deriving them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeededBase {
    /// The branch the seeded root node is bound to — and, through it, the base every descendant's
    /// worktree is created from.
    pub branch: String,
    /// The title the seeded row reads as: the base session's name, or its branch when it has none.
    pub title: String,
    /// The repository the base session records, when it records one. Consulted by callers that must
    /// keep a stack inside one repository; the seeding writer itself has no use for it.
    pub repo_path: Option<String>,
}

/// Why a session cannot be the base of a PR stack.
///
/// Carries the reason **and** which kind of refusal it is, because the daemon answers RPC callers in
/// a status vocabulary (`INVALID_ARGUMENT` for an id that names nothing, `FAILED_PRECONDITION` for a
/// session whose state is wrong) and must not re-derive the rules to pick a code. The message itself
/// is written once, here, so the CLI seeding path and the pre-spawn RPC refusal say the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackSeedBaseRefusal {
    /// The named session does not resolve to a readable changeset: the id itself is wrong.
    Unresolvable(String),
    /// The session resolves, but its state cannot seed a stack.
    Unusable(String),
}

impl StackSeedBaseRefusal {
    /// The operator-facing reason, identical for every caller.
    #[must_use]
    pub fn reason(&self) -> &str {
        match self {
            Self::Unresolvable(reason) | Self::Unusable(reason) => reason,
        }
    }
}

impl std::fmt::Display for StackSeedBaseRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

/// Whether `base_session_id` may be the base of a PR stack, and what it would contribute if it may.
///
/// The base session's half of the seeding preconditions, factored out of
/// [`seed_stack_with_base_session`] so it can be checked **before** anything is written or spawned:
///
/// - the daemon calls it before it spawns an orchestrator, so the new-session form can show the
///   reason instead of navigating to a session that came up unseeded;
/// - `tddy-coder`'s startup gate calls it before it creates the orchestrator's `changeset.yaml`, so a
///   refused seed leaves no changeset behind that would later resume as an ordinary *unseeded*
///   orchestrator;
/// - the seeding writer calls it as its own contract, since the CLI flag is reachable without the RPC.
///
/// The rules it enforces, all of them reads:
///
/// | Condition | Why it is refused |
/// |---|---|
/// | the session does not resolve to a readable changeset | there is nothing to bind a node to |
/// | it records no branch | the branch is the node's whole purpose, and a branchless base fails the spawn gate for every descendant |
/// | it is already a node of another orchestrator's stack | two orchestrators with repoint and pull authority over one branch is ambiguous ownership |
///
/// The caller-specific rules stay with their callers: whether the stack is already populated and
/// whether the base is the orchestrator itself need the orchestrator (see
/// [`check_stack_seed_not_self`]), and whether the repository matches needs the requesting project.
pub fn check_stack_seed_base(
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<SeededBase, StackSeedBaseRefusal> {
    let base_session_id = base_session_id.trim();
    let base_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_root, base_session_id);
    let base = tddy_core::changeset::read_changeset(&base_dir).map_err(|e| {
        StackSeedBaseRefusal::Unresolvable(format!(
            "base session '{base_session_id}' could not be read: {e}"
        ))
    })?;

    let branch = non_blank(base.branch.as_deref())
        .ok_or_else(|| {
            StackSeedBaseRefusal::Unusable(format!(
                "session '{base_session_id}' owns no branch, so there is nothing for the stack's \
                 root node to be bound to"
            ))
        })?
        .to_string();

    if let Some(owner) = non_blank(base.orchestrator_session_id.as_deref()) {
        return Err(StackSeedBaseRefusal::Unusable(format!(
            "session '{base_session_id}' is already a node of the stack orchestrated by '{owner}', \
             so a second stack cannot be based on it — two orchestrators with repoint and pull \
             authority over one branch is ambiguous ownership"
        )));
    }

    // A node with no legible title is the one thing the panel cannot render usefully, so an unnamed
    // base session is titled after the branch it works on.
    let title = non_blank(base.name.as_deref())
        .unwrap_or(branch.as_str())
        .to_string();

    Ok(SeededBase {
        branch,
        title,
        repo_path: non_blank(base.repo_path.as_deref()).map(str::to_string),
    })
}

/// Refuse a stack seeded on the very session that owns it.
///
/// Separate from [`check_stack_seed_base`] because it needs the orchestrator, which the daemon does
/// not have when it validates — the session does not exist yet. Both callers that *do* have it (the
/// seeding writer and `tddy-coder`'s startup gate) share this one wording.
pub fn check_stack_seed_not_self(
    orchestrator_dir: &Path,
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<(), String> {
    let base_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        sessions_root,
        base_session_id.trim(),
    );
    if is_same_session_dir(orchestrator_dir, &base_dir) {
        return Err(format!(
            "refusing to base the stack on itself — '{}' is the session that owns this stack",
            base_session_id.trim()
        ));
    }
    Ok(())
}

/// Append the single root node a new orchestrator's stack is seeded with, bound to a session the
/// operator already has open.
///
/// A **creation-time** act: every other way to populate a stack starts from something that does not
/// exist yet (a plan the agent writes, a planned node with no branch, a pull request), and this one
/// starts from work in flight. It refuses a stack that already has nodes rather than growing one —
/// growing a stack is [`add_planned_pr_node`]'s job, and a second seed would add a duplicate root.
///
/// The node owns a `branch` from the start, so `branch_suggestion` stays `None`: nothing here is
/// going to choose a name. `pr_status` stays `None` — whether that branch has a pull request is the
/// live-status poll's to discover, and recording `planned` for a branch that may already have an
/// open one would misreport it until the first tick.
///
/// Every refusal is raised before the write, so a refused seed leaves the changeset untouched.
///
/// [`add_planned_pr_node`]: super::add_planned_pr_node
pub fn seed_stack_with_base_session(
    orchestrator_dir: &Path,
    sessions_root: &Path,
    base_session_id: &str,
) -> Result<StackNode, String> {
    const OP: &str = "seed_stack_with_base_session";

    check_stack_seed_not_self(orchestrator_dir, sessions_root, base_session_id)
        .map_err(|reason| format!("{OP}: {reason}"))?;

    let existing = read_stack(orchestrator_dir, OP)?;
    if !existing.nodes.is_empty() {
        return Err(format!(
            "{OP}: this stack already has {} node(s), so there is nothing to seed — a stack is \
             grown with add_planned_pr_node",
            existing.nodes.len()
        ));
    }

    // The base session's own half of the preconditions, shared with every other caller that has to
    // decide whether a session can seed a stack — so the rules, and their wording, exist once.
    let base = check_stack_seed_base(sessions_root, base_session_id)
        .map_err(|refusal| format!("{OP}: {refusal}"))?;

    let seeded = StackNode {
        node_id: next_free_node_id(&existing),
        title: base.title,
        description: String::new(),
        branch: Some(base.branch),
        branch_suggestion: None,
        session_id: Some(base_session_id.to_string()),
        // A seeded node is the root of the stack: nothing existed for it to be stacked on.
        parents: Vec::new(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        // Chosen by `append_node_atomic` below, against the stack that is actually about to be
        // written.
        display_order: None,
    };

    append_node_atomic(orchestrator_dir, seeded, OP)
}

/// The trimmed value, or `None` for an absent or blank one — proto3 and a hand-edited changeset both
/// carry "unset" as the empty string.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// Whether two paths name the same session directory.
///
/// Compares the canonical forms as well as the literal ones: the orchestrator's directory reaches
/// [`seed_stack_with_base_session`] as the path its own process was given, which need not be spelled
/// the way `sessions_root` spells it (a symlinked or relative sessions root).
fn is_same_session_dir(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
