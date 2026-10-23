//! **The** resolver from a target session id to the worktree a project-home session's tools act on
//! (`#project-home` 5/7; node 7's git tools use it unchanged).
//!
//! A target must be another session of the caller's own project, never the home session itself,
//! and must have a worktree. The answer carries the target's activity, which is what gates
//! destructive git (node 7).
//!
//! Pure over the records the daemon reads from `.session.yaml`, so the rules are testable without
//! a daemon.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session-tools.md.

use std::fmt;
use std::path::PathBuf;

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
    pub worktree: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub is_active: bool,
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
    let worktree = record
        .worktree
        .clone()
        .ok_or_else(|| SessionTargetError::NoWorktree(target_session_id.to_string()))?;
    Ok(SessionTarget {
        session_id: record.session_id.clone(),
        project_id: record.project_id.clone(),
        worktree,
        branch: record.branch.clone(),
        base_ref: record.base_ref.clone(),
        is_active: record.is_active,
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
                worktree: PathBuf::from("/worktrees/s1"),
                branch: "feature/s1".to_string(),
                base_ref: "origin/main".to_string(),
                is_active: true,
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
    fn resolving_a_session_without_a_worktree_is_refused() {
        let records = vec![TargetRecord {
            worktree: None,
            ..a_record("bare", PROJECT)
        }];
        assert_eq!(
            resolve_session_target(&records, &the_home(), "bare"),
            Err(SessionTargetError::NoWorktree("bare".to_string()))
        );
    }
}
