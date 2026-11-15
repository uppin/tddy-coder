//! **The** resolver from a target session id to the worktree a project-home session's tools act on
//! (`#project-home` 5/7; node 7's git tools use it unchanged).
//!
//! A target must be another session of the caller's own project, never the home session itself.
//! A target without a worktree — a split session's agent half — is still a target for the tools
//! that act on the agent; the tools that need the checkout ask for it with
//! [`SessionTarget::worktree`]. The answer carries the target's activity, which is what gates
//! destructive git (node 7).
//!
//! Pure over the records the daemon reads from `.session.yaml`, so the rules are testable without
//! a daemon.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where a split session's checkout lives: the workspace session holding it, on the daemon that
/// holds it (`.session.yaml`'s `codebase_daemon_instance_id` / `codebase_session_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodebaseHalf {
    pub daemon_instance_id: String,
    pub session_id: String,
}

/// One session as the resolver needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRecord {
    pub session_id: String,
    pub project_id: String,
    /// The session's worktree (`repo_path`); `None` when it has none.
    pub worktree: Option<PathBuf>,
    pub branch: String,
    pub base_ref: String,
    pub is_active: bool,
    /// The codebase half of a split session's agent half; `None` for every other session.
    pub codebase: Option<CodebaseHalf>,
}

/// The project-home session asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeCaller {
    pub session_id: String,
    pub project_id: String,
}

/// A resolved target: everything the tools act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTarget {
    pub session_id: String,
    pub project_id: String,
    /// The target's checkout; `None` for a session without one (a split session's agent half).
    pub worktree: Option<PathBuf>,
    pub branch: String,
    pub base_ref: String,
    pub is_active: bool,
    /// The codebase half of a split session's agent half; `None` for every other session.
    pub codebase: Option<CodebaseHalf>,
}

impl SessionTarget {
    /// The checkout a worktree or git tool acts on, refused for a target that has none.
    pub fn worktree(&self) -> Result<&Path, SessionTargetError> {
        self.worktree
            .as_deref()
            .ok_or_else(|| SessionTargetError::NoWorktree(self.session_id.clone()))
    }
}

/// Why a target id was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTargetError {
    NotFound(String),
    OtherProject(String),
    IsHome,
    NoWorktree(String),
}

impl fmt::Display for SessionTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "no session {id}"),
            Self::OtherProject(id) => write!(f, "session {id} belongs to another project"),
            Self::IsHome => f.write_str("the project-home session is not a target"),
            Self::NoWorktree(id) => write!(f, "session {id} has no worktree"),
        }
    }
}

impl std::error::Error for SessionTargetError {}

/// Resolve `target_session_id` for `home` among `records`.
pub fn resolve_session_target(
    records: &[TargetRecord],
    home: &HomeCaller,
    target_session_id: &str,
) -> Result<SessionTarget, SessionTargetError> {
    if target_session_id == home.session_id {
        return Err(SessionTargetError::IsHome);
    }
    let record = records
        .iter()
        .find(|r| r.session_id == target_session_id)
        .ok_or_else(|| SessionTargetError::NotFound(target_session_id.to_string()))?;
    if record.project_id != home.project_id {
        return Err(SessionTargetError::OtherProject(
            target_session_id.to_string(),
        ));
    }
    // TODO(split-targets): admit a target without a worktree — the tools that need one ask
    // `SessionTarget::worktree`.
    if record.worktree.is_none() {
        return Err(SessionTargetError::NoWorktree(
            target_session_id.to_string(),
        ));
    }
    Ok(SessionTarget {
        session_id: record.session_id.clone(),
        project_id: record.project_id.clone(),
        worktree: record.worktree.clone(),
        branch: record.branch.clone(),
        base_ref: record.base_ref.clone(),
        is_active: record.is_active,
        codebase: record.codebase.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "proj-a";

    fn a_record(session_id: &str, project_id: &str) -> TargetRecord {
        TargetRecord {
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            worktree: Some(PathBuf::from(format!("/worktrees/{session_id}"))),
            branch: format!("feature/{session_id}"),
            base_ref: "origin/main".to_string(),
            is_active: true,
            codebase: None,
        }
    }

    fn the_home() -> HomeCaller {
        HomeCaller {
            session_id: "home".to_string(),
            project_id: PROJECT.to_string(),
        }
    }

    #[test]
    fn resolving_a_session_of_the_same_project_returns_its_worktree_and_activity() {
        // Given
        let records = vec![a_record("home", PROJECT), a_record("s1", PROJECT)];

        // When
        let target = resolve_session_target(&records, &the_home(), "s1");

        // Then
        assert_eq!(
            target,
            Ok(SessionTarget {
                session_id: "s1".to_string(),
                project_id: PROJECT.to_string(),
                worktree: Some(PathBuf::from("/worktrees/s1")),
                branch: "feature/s1".to_string(),
                base_ref: "origin/main".to_string(),
                is_active: true,
                codebase: None,
            })
        );
    }

    #[test]
    fn resolving_the_home_session_itself_is_refused() {
        let records = vec![a_record("home", PROJECT)];
        assert_eq!(
            resolve_session_target(&records, &the_home(), "home"),
            Err(SessionTargetError::IsHome)
        );
    }

    #[test]
    fn resolving_an_unknown_session_is_not_found() {
        let records = vec![a_record("home", PROJECT)];
        assert_eq!(
            resolve_session_target(&records, &the_home(), "ghost"),
            Err(SessionTargetError::NotFound("ghost".to_string()))
        );
    }

    #[test]
    fn resolving_a_session_of_another_project_is_refused() {
        let records = vec![a_record("home", PROJECT), a_record("elsewhere", "proj-b")];
        assert_eq!(
            resolve_session_target(&records, &the_home(), "elsewhere"),
            Err(SessionTargetError::OtherProject("elsewhere".to_string()))
        );
    }

    #[test]
    fn resolving_a_session_without_a_worktree_targets_it() {
        // Given a session without a worktree — the agent half of a split session
        let records = vec![TargetRecord {
            worktree: None,
            ..a_record("bare", PROJECT)
        }];

        // When it is resolved
        let target = resolve_session_target(&records, &the_home(), "bare");

        // Then it is a target: the tools that act on the agent need no checkout
        assert_eq!(
            target.map(|t| (t.session_id, t.worktree)),
            Ok(("bare".to_string(), None))
        );
    }

    #[test]
    fn a_target_without_a_worktree_refuses_to_give_one() {
        // Given a target without a worktree
        let target = SessionTarget {
            session_id: "bare".to_string(),
            project_id: PROJECT.to_string(),
            worktree: None,
            branch: String::new(),
            base_ref: "origin/main".to_string(),
            is_active: false,
            codebase: None,
        };

        // When a worktree tool asks for its checkout
        let worktree = target.worktree();

        // Then it is refused, naming the session
        assert_eq!(
            worktree,
            Err(SessionTargetError::NoWorktree("bare".to_string()))
        );
    }

    #[test]
    fn a_split_agent_record_names_its_codebase_half() {
        // Given the agent half of a split session, whose checkout is on another host
        let codebase = CodebaseHalf {
            daemon_instance_id: "udoo".to_string(),
            session_id: "agent-cb".to_string(),
        };
        let records = vec![TargetRecord {
            worktree: None,
            codebase: Some(codebase.clone()),
            ..a_record("agent", PROJECT)
        }];

        // When it is resolved
        let target = resolve_session_target(&records, &the_home(), "agent");

        // Then the target names where its checkout is
        assert_eq!(target.map(|t| t.codebase), Ok(Some(codebase)));
    }
}
