//! `EnsureProjectHomeSession`: start, resume, or return a project's project-home session
//! (`#project-home` 4/7).
//!
//! The decision, by what exists for the project on this host:
//! - no home agent set, or one whose harness cannot enforce managed-codebase mode → refused;
//! - no project-home session → create the home worktree and start a managed session on it with the
//!   home agent and the `project-home` role ([`EnsureOutcome::Started`]);
//! - a stopped one → fast-forward the home worktree, then resume it, same conversation
//!   ([`EnsureOutcome::Resumed`]);
//! - a running one → return it unchanged ([`EnsureOutcome::Running`]).
//!
//! It builds a `StartSessionRequest` and calls the existing start and resume paths; it adds no
//! branch to `start_session_core` (`docs/code-issues/complexity-svc-start-session-core-*`).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md.

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse,
};

use crate::launch_ports::LaunchSessions;

/// What `EnsureProjectHomeSession` did, reported on the wire as `outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureOutcome {
    Started,
    Resumed,
    Running,
}

impl EnsureOutcome {
    /// The wire spelling of this outcome.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Resumed => "resumed",
            Self::Running => "running",
        }
    }
}

impl LaunchSessions {
    pub async fn ensure_project_home_session_at_session_coordinate(
        &self,
        request: Request<EnsureProjectHomeSessionRequest>,
    ) -> Result<Response<EnsureProjectHomeSessionResponse>, Status> {
        // TODO(home-session): implement
        let _ = request;
        Err(Status::unimplemented(
            "EnsureProjectHomeSession is not implemented",
        ))
    }
}
