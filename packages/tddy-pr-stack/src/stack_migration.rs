//! The startup sweep that moves every session-held PR stack into its project.
//!
//! Before `#orchestrators` 2/11 a stack lived in the planning session's `changeset.yaml`
//! (`Changeset.stack`). The sweep gives each such stack a project record
//! ([`tddy_core::project_stack_store::ProjectStackStore`]), then writes the record's id into the
//! changeset and drops the stack — in that order, so a sweep interrupted between the two finds the
//! record by `created_by_session_id` on its rerun instead of creating a second one.
//!
//! The daemon runs it at startup, before it serves, over its data directory — the one sessions base
//! every OS user it serves resolves to (each daemon resolver passes that directory as
//! `sessions_base_for_user`'s `config_dir`, which wins over the per-user `$HOME/.tddy` default).
//! The body lives here because `tddy-daemon` is wiring only.

use std::path::Path;

use tddy_core::changeset::{read_changeset, write_changeset_atomic};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::read_session_metadata;

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Sessions whose stack this run moved into their project, sorted.
    pub migrated_session_ids: Vec<String>,
    /// Sessions carrying a stack whose `.session.yaml` names no project (or is missing), left as
    /// they are, sorted.
    pub sessions_without_project: Vec<String>,
    /// Sessions the sweep could not migrate, with why.
    pub failures: Vec<MigrationFailure>,
}

/// One session the sweep could not migrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFailure {
    pub session_id: String,
    pub message: String,
}

/// Move every `Changeset.stack` under `<sessions_base>/sessions/*` into its project's stacks under
/// `projects_dir`. Idempotent: a session already holding only a `stack_id` is not touched.
///
/// Logs what it did — a summary of the sessions moved, and each session left alone or failed — so
/// the caller need not.
pub fn migrate_session_stacks(sessions_base: &Path, projects_dir: &Path) -> MigrationReport {
    let mut report = MigrationReport::default();
    let store = ProjectStackStore::new(projects_dir);
    for session_id in session_ids_under(&sessions_base.join("sessions")) {
        let session_dir = unified_session_dir_path(sessions_base, &session_id);
        let outcome = migrate_one(&store, &session_dir, &session_id);
        report.record(session_id, outcome);
    }
    if !report.migrated_session_ids.is_empty() {
        log::info!(
            target: "tddy_pr_stack::stack_migration",
            "moved the PR stack of {} session(s) into their projects: {}",
            report.migrated_session_ids.len(),
            report.migrated_session_ids.join(", ")
        );
    }
    report
}

/// The session directories under `sessions_dir`, sorted; none when it does not exist.
fn session_ids_under(sessions_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(sessions_dir) else {
        return Vec::new();
    };
    let mut session_ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    session_ids.sort();
    session_ids
}

impl MigrationReport {
    /// File one session's outcome, logging the sessions the sweep left alone or could not move.
    fn record(&mut self, session_id: String, outcome: Result<Outcome, String>) {
        match outcome {
            Ok(Outcome::Untouched) => {}
            Ok(Outcome::Migrated) => self.migrated_session_ids.push(session_id),
            Ok(Outcome::NoProject) => {
                log::warn!(
                    target: "tddy_pr_stack::stack_migration",
                    "session {session_id} carries a stack but records no project; left as it is"
                );
                self.sessions_without_project.push(session_id);
            }
            Err(message) => {
                log::error!(
                    target: "tddy_pr_stack::stack_migration",
                    "session {session_id}: stack migration failed: {message}"
                );
                self.failures.push(MigrationFailure {
                    session_id,
                    message,
                });
            }
        }
    }
}

enum Outcome {
    Untouched,
    Migrated,
    NoProject,
}

fn migrate_one(
    store: &ProjectStackStore,
    session_dir: &Path,
    session_id: &str,
) -> Result<Outcome, String> {
    if !session_dir.join("changeset.yaml").is_file() {
        return Ok(Outcome::Untouched);
    }
    let changeset = read_changeset(session_dir).map_err(|e| e.to_string())?;
    let Some(legacy_stack) = changeset.stack else {
        return Ok(Outcome::Untouched);
    };
    let project_id = read_session_metadata(session_dir)
        .map(|m| m.project_id)
        .unwrap_or_default();
    if project_id.trim().is_empty() {
        return Ok(Outcome::NoProject);
    }
    let stack_id = match changeset.stack_id {
        Some(stored) => {
            store.read(&project_id, &stored).map_err(|e| e.to_string())?;
            stored
        }
        None => record_for(store, &project_id, session_id, legacy_stack)?,
    };

    let mut migrated = read_changeset(session_dir).map_err(|e| e.to_string())?;
    migrated.stack = None;
    migrated.stack_id = Some(stack_id);
    write_changeset_atomic(session_dir, &migrated).map_err(|e| e.to_string())?;
    Ok(Outcome::Migrated)
}

/// The id of the record holding `session_id`'s stack: the one an interrupted earlier sweep wrote
/// (found by `created_by_session_id`), else a new one. A changeset that already names its record
/// keeps that record instead — it may have been updated since the stale copy was left — so this is
/// reached only for a changeset with no `stack_id`.
fn record_for(
    store: &ProjectStackStore,
    project_id: &str,
    session_id: &str,
    legacy_stack: tddy_core::changeset::Stack,
) -> Result<String, String> {
    let existing = store
        .list(project_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.created_by_session_id == session_id);
    match existing {
        Some(record) => Ok(record.stack_id),
        None => store
            .create(project_id, session_id, legacy_stack)
            .map(|record| record.stack_id)
            .map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tddy_core::changeset::{read_changeset, write_changeset, Changeset, Stack, StackNode};
    use tddy_core::project_stack_store::ProjectStackStore;
    use tddy_core::session_lifecycle::unified_session_dir_path;
    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };

    fn a_stack(node_ids: &[&str]) -> Stack {
        Stack {
            version: 1,
            nodes: node_ids
                .iter()
                .map(|id| StackNode {
                    node_id: id.to_string(),
                    title: format!("PR {id}"),
                    ..StackNode::default()
                })
                .collect(),
        }
    }

    struct AHost {
        root: tempfile::TempDir,
    }

    fn a_host() -> AHost {
        AHost {
            root: tempfile::tempdir().expect("temp sessions base"),
        }
    }

    impl AHost {
        fn projects_dir(&self) -> PathBuf {
            self.root.path().join("projects")
        }

        fn store(&self) -> ProjectStackStore {
            ProjectStackStore::new(self.projects_dir())
        }

        fn with_a_session(
            &self,
            session_id: &str,
            project_id: &str,
            changeset: Changeset,
        ) -> PathBuf {
            let dir = unified_session_dir_path(self.root.path(), session_id);
            std::fs::create_dir_all(&dir).expect("session dir");
            write_initial_tool_session_metadata(
                &dir,
                InitialToolSessionMetadataOpts {
                    project_id: project_id.to_string(),
                    ..InitialToolSessionMetadataOpts::default()
                },
            )
            .expect("write .session.yaml");
            write_changeset(&dir, &changeset).expect("write changeset.yaml");
            dir
        }

        fn with_a_legacy_orchestrator(
            &self,
            session_id: &str,
            project_id: &str,
            stack: Stack,
        ) -> PathBuf {
            self.with_a_session(
                session_id,
                project_id,
                Changeset {
                    recipe: Some("pr-stack".to_string()),
                    stack: Some(stack),
                    ..Changeset::default()
                },
            )
        }

        fn sweep(&self) -> MigrationReport {
            migrate_session_stacks(self.root.path(), &self.projects_dir())
        }
    }

    #[test]
    fn a_host_with_no_sessions_directory_reports_nothing() {
        // Given
        let host = a_host();

        // When
        let report = host.sweep();

        // Then
        assert_eq!(report, MigrationReport::default());
    }

    #[test]
    fn a_session_without_a_stack_is_not_touched() {
        // Given
        let host = a_host();
        let session = host.with_a_session("sess-plain", "proj-auth", Changeset::default());
        let before = std::fs::read_to_string(session.join("changeset.yaml")).unwrap();

        // When
        let report = host.sweep();

        // Then
        assert_eq!(report, MigrationReport::default());
        assert_eq!(
            std::fs::read_to_string(session.join("changeset.yaml")).unwrap(),
            before
        );
        assert!(
            !host.projects_dir().exists(),
            "no project record for a stackless session"
        );
    }

    #[test]
    fn a_session_with_no_changeset_is_neither_migrated_nor_reported() {
        // Given — a CLI harness session: metadata, no workflow manifest
        let host = a_host();
        let dir = unified_session_dir_path(host.root.path(), "sess-cli");
        std::fs::create_dir_all(&dir).expect("session dir");
        write_initial_tool_session_metadata(
            &dir,
            InitialToolSessionMetadataOpts {
                project_id: "proj-auth".to_string(),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write .session.yaml");

        // When
        let report = host.sweep();

        // Then
        assert_eq!(report, MigrationReport::default());
        assert!(
            !dir.join("changeset.yaml").exists(),
            "no changeset may be invented"
        );
    }

    #[test]
    fn every_legacy_orchestrator_is_migrated_and_reported_in_session_id_order() {
        // Given
        let host = a_host();
        host.with_a_legacy_orchestrator("sess-b", "proj-auth", a_stack(&["n1"]));
        host.with_a_legacy_orchestrator("sess-a", "proj-billing", a_stack(&["m1"]));

        // When
        let report = host.sweep();

        // Then
        assert_eq!(
            report.migrated_session_ids,
            vec!["sess-a".to_string(), "sess-b".to_string()]
        );
        assert_eq!(
            (
                host.store().list("proj-auth").expect("list auth").len(),
                host.store()
                    .list("proj-billing")
                    .expect("list billing")
                    .len()
            ),
            (1, 1)
        );
    }

    #[test]
    fn a_changeset_still_carrying_its_stack_beside_a_stored_id_keeps_the_stored_copy() {
        // Given — interrupted after `stack_id` was written but before `stack` was dropped, with
        // the stored copy updated since
        let host = a_host();
        let stored = host
            .store()
            .create("proj-auth", "sess-planner", a_stack(&["n1", "n2"]))
            .expect("stored stack");
        let session = host.with_a_session(
            "sess-planner",
            "proj-auth",
            Changeset {
                recipe: Some("pr-stack".to_string()),
                stack: Some(a_stack(&["n1"])),
                stack_id: Some(stored.stack_id.clone()),
                ..Changeset::default()
            },
        );

        // When
        host.sweep();

        // Then — the stale session copy is dropped; the stored record is not overwritten
        let migrated = read_changeset(&session).expect("read changeset");
        assert_eq!(
            (migrated.stack, migrated.stack_id),
            (None, Some(stored.stack_id.clone()))
        );
        assert_eq!(
            host.store()
                .read("proj-auth", &stored.stack_id)
                .expect("read stored")
                .stack,
            a_stack(&["n1", "n2"])
        );
    }

    #[test]
    fn a_session_whose_changeset_does_not_parse_is_reported_as_a_failure() {
        // Given
        let host = a_host();
        let session = host.with_a_legacy_orchestrator("sess-broken", "proj-auth", a_stack(&["n1"]));
        std::fs::write(session.join("changeset.yaml"), "state: [not a changeset").unwrap();

        // When
        let report = host.sweep();

        // Then
        assert_eq!(
            report
                .failures
                .iter()
                .map(|f| f.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["sess-broken"]
        );
    }
}
