//! Crash-safe journal for in-flight merge+repoint operations.

use std::path::Path;

/// Phase of the merge+repoint atomic operation for one node.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "phase")]
pub enum MergePhase {
    Planned,
    PrMerged { sha: String },
    RepointingDependent { idx: usize },
    Done,
}

/// Journal written to disk before each destructive step, enabling crash recovery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StackOpJournal {
    pub op_id: String,
    pub merged_node_id: String,
    pub merge_phase: MergePhase,
    pub dependents: Vec<String>,
}

const JOURNAL_SUBDIR: &str = ".workflow";
const JOURNAL_FILENAME: &str = "stack-op.json";

fn journal_path(parent_dir: &Path) -> std::path::PathBuf {
    parent_dir.join(JOURNAL_SUBDIR).join(JOURNAL_FILENAME)
}

/// Write a journal to `parent_dir/.workflow/stack-op.json` atomically (temp + rename).
pub fn write_stack_op_journal(
    parent_dir: &Path,
    journal: &StackOpJournal,
) -> Result<(), tddy_core::WorkflowError> {
    let dir = parent_dir.join(JOURNAL_SUBDIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| tddy_core::WorkflowError::WriteFailed(e.to_string()))?;
    let content = serde_json::to_string_pretty(journal)
        .map_err(|e| tddy_core::WorkflowError::WriteFailed(e.to_string()))?;
    let tmp = dir.join(format!("{}.tmp", JOURNAL_FILENAME));
    std::fs::write(&tmp, &content)
        .map_err(|e| tddy_core::WorkflowError::WriteFailed(e.to_string()))?;
    std::fs::rename(&tmp, journal_path(parent_dir))
        .map_err(|e| tddy_core::WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read the in-flight journal from `parent_dir/.workflow/stack-op.json` (if present).
/// Returns `Ok(None)` when no journal exists or when phase is `Done`.
/// When phase is `PrMerged`, advances to `RepointingDependent { idx: 0 }` and writes the
/// updated journal before returning so recovery is idempotent on restart.
pub fn recover_in_flight_stack_op(
    parent_dir: &Path,
) -> Result<Option<StackOpJournal>, tddy_core::WorkflowError> {
    let path = journal_path(parent_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| tddy_core::WorkflowError::ChangesetInvalid(e.to_string()))?;
    let mut journal: StackOpJournal = serde_json::from_str(&content)
        .map_err(|e| tddy_core::WorkflowError::ChangesetInvalid(e.to_string()))?;
    match &journal.merge_phase {
        MergePhase::Done => Ok(None),
        MergePhase::PrMerged { .. } => {
            journal.merge_phase = MergePhase::RepointingDependent { idx: 0 };
            write_stack_op_journal(parent_dir, &journal)?;
            Ok(Some(journal))
        }
        _ => Ok(Some(journal)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_phase_serde_round_trip() {
        let phase = MergePhase::PrMerged { sha: "abc123".to_string() };
        let json = serde_json::to_string(&phase).expect("MergePhase should serialize");
        let back: MergePhase = serde_json::from_str(&json).expect("MergePhase should deserialize");
        assert_eq!(back, phase);
    }

    #[test]
    fn write_stack_op_journal_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = StackOpJournal {
            op_id: "op-1".to_string(),
            merged_node_id: "n1".to_string(),
            merge_phase: MergePhase::Planned,
            dependents: vec![],
        };
        write_stack_op_journal(tmp.path(), &journal).unwrap();
        assert!(tmp.path().join(".workflow").join("stack-op.json").exists());
    }

    #[test]
    fn recover_in_flight_stack_op_returns_none_when_no_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let result = recover_in_flight_stack_op(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn recover_in_flight_stack_op_advances_pr_merged_phase() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = StackOpJournal {
            op_id: "op-1".to_string(),
            merged_node_id: "n1".to_string(),
            merge_phase: MergePhase::PrMerged { sha: "abc".to_string() },
            dependents: vec!["n2".to_string()],
        };
        write_stack_op_journal(tmp.path(), &journal).unwrap();
        let recovered = recover_in_flight_stack_op(tmp.path()).unwrap().unwrap();
        assert_eq!(recovered.merged_node_id, "n1");
        assert!(matches!(recovered.merge_phase, MergePhase::RepointingDependent { idx: 0 }));
    }

    #[test]
    fn recover_in_flight_stack_op_returns_none_when_done() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = StackOpJournal {
            op_id: "op-1".to_string(),
            merged_node_id: "n1".to_string(),
            merge_phase: MergePhase::Done,
            dependents: vec![],
        };
        write_stack_op_journal(tmp.path(), &journal).unwrap();
        let result = recover_in_flight_stack_op(tmp.path()).unwrap();
        assert!(result.is_none());
    }
}

/// Delete the journal file (call after a successful complete operation).
pub fn delete_stack_op_journal(parent_dir: &Path) -> Result<(), std::io::Error> {
    let path = parent_dir.join(".workflow").join("stack-op.json");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
