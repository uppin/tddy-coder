//! Reading and writing `changeset.yaml`.
//!
//! Every write is atomic: a half-written manifest is unreadable to every later goal in the
//! session, and no call site wants the truncating variant.

use super::model::Changeset;
use crate::error::WorkflowError;
use log::info;
use std::fs;
use std::path::Path;

/// Read changeset from plan directory.
pub fn read_changeset(session_dir: &Path) -> Result<Changeset, WorkflowError> {
    let path = session_dir.join("changeset.yaml");
    let content = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkflowError::ChangesetMissing(format!(
                "changeset.yaml not found in {} — run the plan goal first",
                session_dir.display()
            ))
        } else {
            WorkflowError::ChangesetInvalid(e.to_string())
        }
    })?;
    serde_yaml::from_str(&content).map_err(|e| WorkflowError::ChangesetInvalid(e.to_string()))
}

/// Write changeset to plan directory.
///
/// Atomic, like [`write_changeset_atomic`] — both names now mean the same guarantee, because a
/// half-written `changeset.yaml` is unreadable to every later goal in the session and there is no
/// call site that wants the truncating variant.
pub fn write_changeset(session_dir: &Path, changeset: &Changeset) -> Result<(), WorkflowError> {
    write_changeset_atomic(session_dir, changeset)
}

/// Ensures the session's changeset carries `recipe_name` — creates changeset.yaml with this
/// recipe if none exists yet, fills in `recipe` if the existing changeset has it unset, and
/// leaves an already-set (possibly different) recipe untouched.
pub fn ensure_changeset_recipe(session_dir: &Path, recipe_name: &str) -> Result<(), WorkflowError> {
    let mut cs = match read_changeset(session_dir) {
        Ok(cs) => cs,
        Err(WorkflowError::ChangesetMissing(_)) => Changeset::default(),
        Err(e) => return Err(e),
    };
    if cs.recipe.is_some() {
        return Ok(());
    }
    cs.recipe = Some(recipe_name.to_string());
    write_changeset(session_dir, &cs)
}

/// Atomically replace `changeset.yaml` (write swap file + rename) so readers never see a partial
/// file, and a write that runs out of disk leaves the previous changeset standing.
pub fn write_changeset_atomic(
    session_dir: &Path,
    changeset: &Changeset,
) -> Result<(), WorkflowError> {
    info!(
        target: "tddy_core::changeset",
        "write_changeset_atomic: session_dir={}",
        session_dir.display()
    );
    let path = session_dir.join("changeset.yaml");
    let content =
        serde_yaml::to_string(changeset).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    crate::atomic_file::write_atomic_labelled(&path, content).map_err(WorkflowError::WriteFailed)
}

#[cfg(test)]
mod ensure_changeset_recipe_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_a_changeset_with_the_recipe_when_none_exists_yet() {
        // Given — a session directory with no changeset.yaml at all (the daemon has allocated
        // the directory but the workflow hasn't written anything into it yet).
        let dir = tempdir().expect("tempdir");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then
        let cs = read_changeset(dir.path()).expect("changeset.yaml must now exist");
        assert_eq!(cs.recipe, Some("plan-pr-stack".to_string()));
    }

    #[test]
    fn fills_in_the_recipe_on_an_existing_changeset_that_has_none_without_losing_other_fields() {
        // Given — a changeset already on disk (e.g. written by the daemon's session-metadata
        // step) that has no recipe set, but does carry other real data.
        let dir = tempdir().expect("tempdir");
        write_changeset(
            dir.path(),
            &Changeset {
                initial_prompt: Some("plan a 3-PR todo app stack".to_string()),
                recipe: None,
                ..Changeset::default()
            },
        )
        .expect("write_changeset");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then
        let cs = read_changeset(dir.path()).expect("read_changeset");
        assert_eq!(cs.recipe, Some("plan-pr-stack".to_string()));
        assert_eq!(
            cs.initial_prompt,
            Some("plan a 3-PR todo app stack".to_string()),
            "existing fields must survive filling in the recipe"
        );
    }

    #[test]
    fn does_not_overwrite_an_already_set_different_recipe() {
        // Given — a resumed session whose changeset already has a (different) recipe recorded.
        let dir = tempdir().expect("tempdir");
        write_changeset(
            dir.path(),
            &Changeset {
                recipe: Some("tdd".to_string()),
                ..Changeset::default()
            },
        )
        .expect("write_changeset");

        // When
        ensure_changeset_recipe(dir.path(), "plan-pr-stack").expect("ensure_changeset_recipe");

        // Then — the existing recipe is left untouched, not clobbered by the new call.
        let cs = read_changeset(dir.path()).expect("read_changeset");
        assert_eq!(cs.recipe, Some("tdd".to_string()));
    }
}
