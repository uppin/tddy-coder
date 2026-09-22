//! The PR-stack data model and its git/GitHub operations.
//!
//! Everything here reads and writes a session's `Changeset.stack` (from `tddy-core`), drives `git`
//! (through `tddy-git`), or talks to the GitHub REST API (through `tddy-github`). None of it is a
//! workflow recipe: the `pr-stack` recipe, its hooks, and the plan→stack bridges live in
//! `tddy-workflow-recipes`, which re-exports every item below from its historical path.
//!
//! [`rpc`] is the one transport-facing module: it holds the `pr_stack.PrStackService` handler trait
//! and adapter, so the session lifecycle can name the trait without depending on the daemon crate
//! that implements it.

pub mod assess;
pub mod docs;
pub mod git_ops;
pub mod pr_insight;
pub mod rpc;
pub mod stack_ops;

pub use rpc::PR_STACK_SERVICE;
