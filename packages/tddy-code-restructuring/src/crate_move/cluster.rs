use crate::crate_move::destination;
use crate::plan::RefactorOp;

use crate::edit::WorkspaceEdit;

use super::Result;

use crate::registry::Workspace;

use super::ModuleReferences;

use std::collections::BTreeSet;

use crate::plan::Reexport;

use super::module_home;

/// A set of modules that move to one destination **as a single unit**.
///
/// `move_module_to_crate` models one module, and that is why a mutually-referencing subsystem
/// cannot move: the header pass re-points every `crate::` path at the *origin*, so a reference to a
/// sibling that is also moving becomes a `destination → origin` edge; and the reference survey runs
/// against a tree where the siblings have not moved, so moving one rewrites the others' callers
/// before they are correct. Between the first operation and the last the tree does not compile,
/// which is why there is no intermediate state to verify against.
///
/// `#unbundle` node 3 moved **0 of 4** entangled modules for exactly this reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovingCluster {
    /// Every module in the set, in the order the plan named them.
    pub members: Vec<module_home::ModuleHome>,
    /// Where they are all going.
    pub destination: destination::Destination,
    /// What each leaves behind in the crate it left.
    pub reexport: Reexport,
}

impl MovingCluster {
    /// The module paths moving together, as a `crate::`-relative path would write them.
    ///
    /// This is what the header pass needs in order to tell a sibling that is coming along from one
    /// that is staying behind — the distinction the single-module model cannot express.
    #[must_use]
    pub fn co_moving(&self) -> BTreeSet<String> {
        self.members
            .iter()
            .map(|member| member.path.join("::"))
            .collect()
    }
}

/// Resolve a whole cluster into one edit, applied all or not at all.
///
/// Every member's callers are surveyed against the **post-move** shape of the set, so a reference to
/// a co-moving sibling is re-pointed at the destination rather than at the crate it left. The
/// returned edit is the union: there is no ordering in which the tree is half-moved.
///
/// # Errors
///
/// Refuses for every reason a single move does, plus: a member that names a crate outside the set
/// which the destination cannot depend on, and a set whose members do not share one destination.
pub fn resolve_cluster(
    _engine: &mut dyn ModuleReferences,
    _workspace: &Workspace<'_>,
    _cluster: &MovingCluster,
) -> Result<WorkspaceEdit> {
    // TODO(restructure-clusters): implement
    todo!("resolve_cluster: move a mutually-referencing set as one unit")
}

/// The modules a plan's cross-crate moves leave behind that still reference what moved.
///
/// The cluster defect's worse half is that `check` cannot see it: a four-operation plan reported
/// `no findings` and was then rejected by `apply`. This names, for a plan that moves some of a
/// mutually-referencing set, the members it left behind — which is the finding that would have made
/// that plan legible before it ran.
///
/// Empty when every move's siblings either come along or do not reference it.
///
/// # Errors
///
/// Refuses when a file the check must read cannot be.
pub fn siblings_left_behind(
    _workspace: &Workspace<'_>,
    _ops: &[RefactorOp],
) -> Result<Vec<String>> {
    // TODO(restructure-clusters): implement
    todo!("siblings_left_behind: name the members a partial cluster move would strand")
}
