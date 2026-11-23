//! Per-PR documents: where a node's document pair lives on disk.
//!
//! PRD: `docs/ft/coder/pr-stack-docs.md`.
//!
//! Each planned PR owns two documents under `artifacts/prs/<node_id>/`: a `PRD.md` saying what the
//! PR delivers, and a `changeset.md` saying where its edges are. Paths are derived from `node_id`
//! by convention rather than recorded on `WorkItem`, so they are predictable by construction.

use std::path::{Path, PathBuf};

/// Subdirectory of `artifacts/` holding the per-PR document directories.
pub const NODE_DOCS_SUBDIR: &str = "prs";

/// The high-level document: what this PR delivers.
pub const NODE_PRD_BASENAME: &str = "PRD.md";

/// The technical document: responsibility, boundaries, dependencies, draft-PR contract.
pub const NODE_CHANGESET_BASENAME: &str = "changeset.md";

/// The human-readable rendering of a stack plan. A persisted artifact name, so it is renamed only
/// with a migration.
pub const PR_STACK_PLAN_MD_BASENAME: &str = "pr-stack-plan.md";

/// Where a node's two documents live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDocPaths {
    pub prd: PathBuf,
    pub changeset: PathBuf,
}

/// `session_dir/artifacts/prs/<node_id>/{PRD.md,changeset.md}` — pure path math, touches no
/// filesystem and enforces no policy.
pub fn node_doc_paths(session_dir: &Path, node_id: &str) -> NodeDocPaths {
    let node_dir = tddy_workflow::session_artifacts_root(session_dir)
        .join(NODE_DOCS_SUBDIR)
        .join(node_id);
    NodeDocPaths {
        prd: node_dir.join(NODE_PRD_BASENAME),
        changeset: node_dir.join(NODE_CHANGESET_BASENAME),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nodes_documents_resolve_under_its_own_directory() {
        // Given
        let session_dir = Path::new("/sessions/abc");

        // When
        let paths = node_doc_paths(session_dir, "n1");

        // Then
        assert_eq!(
            paths,
            NodeDocPaths {
                prd: PathBuf::from("/sessions/abc/artifacts/prs/n1/PRD.md"),
                changeset: PathBuf::from("/sessions/abc/artifacts/prs/n1/changeset.md"),
            }
        );
    }

    #[test]
    fn two_nodes_resolve_to_separate_directories() {
        // Given
        let session_dir = Path::new("/sessions/abc");

        // When
        let first = node_doc_paths(session_dir, "n1");
        let second = node_doc_paths(session_dir, "n2");

        // Then — a node reading its neighbour's boundaries as its own is the failure mode
        assert_ne!(first.changeset, second.changeset);
    }
}
