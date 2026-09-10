//! The daemon's sandbox orchestration: starting a jailed session, provisioning the workspace tool
//! sandbox, and the plans that describe both.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 3. It has the **best test locality in that
//! crate** — 62 lines of inline `#[cfg(test)]` against 5,494 lines of dedicated integration suites
//! — which is the strongest possible position from which to move code, because the tests that prove
//! the behaviour move with it unrewritten.
//!
//! # Why this is a crate and `tddy-spawn` is another
//!
//! Discovery refuted the assumption that sandbox and spawn are one cluster. They share **no**
//! `crate::` edge — the sandbox modules never touch `spawner` — and their dependency sets are
//! disjoint: six `tddy-sandbox*` crates here against `tddy-supervisor` there. Confinement and
//! privileged fork are different concerns, and one crate would have forced every sandbox consumer to
//! link the supervisor.
//!
//! # What this move proves
//!
//! `tddy-sandbox-app` consumed `tddy_daemon::{sandbox_session, tool_engine}` before this node. It
//! now depends on **this** crate and not on `tddy-daemon` at all — a dependency **reversal**, and
//! the node's most checkable outcome. It is asserted in
//! `tests/sandbox_app_dependency_reverses.rs`, not merely described.
//!
//! The two symbols this subsystem reached the 23,099-line god module for —
//! [`tddy_daemon_kernel::AgentActivityHub`] and [`tddy_daemon_kernel::now_unix_ms`] — are node 1's,
//! and they are the whole reason this can leave.
//!
//! # The one edge that could not simply move
//!
//! [`sandbox_session::SandboxSessionState`] carried a
//! `tddy_daemon::session_toolcall::ManagedWorkflow`, which stays in `tddy-daemon` for node 8 — so
//! importing it here would be a cycle. It was never *called*: the field is a drop-carrier, held only
//! so the workflow's socket is cleaned up when the session ends. It is therefore held as
//! [`sandbox_session::SessionScopedResource`], a `Send + Sync` marker the daemon hands its concrete
//! value to. `Box<dyn Trait>` drops through the vtable, so the destructor that runs is still
//! `ManagedWorkflow`'s.

pub mod sandbox_action;
pub mod sandbox_plan_builder;
pub mod sandbox_runtime;
pub mod sandbox_session;
pub mod workspace_tool_sandbox;
