//! The PR-stack data model and its git/GitHub operations.
//!
//! Everything here reads and writes a session's `Changeset.stack` (from `tddy-core`), drives `git`
//! (through `tddy-git`), or talks to the GitHub REST API (through `tddy-github`). None of it is a
//! workflow recipe: the `pr-stack` recipe, its hooks, and the plan→stack bridges live in
//! `tddy-workflow-recipes`, which re-exports every item below from its historical path.

pub mod assess;
pub mod docs;
pub mod git_ops;
pub mod pr_insight;
pub mod stack_ops;
