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

/// Read the in-flight journal from `parent_dir/.workflow/stack-op.json` (if present).
/// Returns `Ok(None)` when no journal exists; `Ok(Some(...))` when one is found.
pub fn recover_in_flight_stack_op(
    parent_dir: &Path,
) -> Result<Option<StackOpJournal>, tddy_core::WorkflowError> {
    unimplemented!("recover_in_flight_stack_op: not yet implemented")
}

/// Write a journal to `parent_dir/.workflow/stack-op.json`.
pub fn write_stack_op_journal(
    parent_dir: &Path,
    journal: &StackOpJournal,
) -> Result<(), tddy_core::WorkflowError> {
    unimplemented!("write_stack_op_journal: not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "write_stack_op_journal")]
    fn write_stack_op_journal_panics_unimplemented() {
        let tmp = tempfile::tempdir().unwrap();
        let journal = StackOpJournal {
            op_id: "op-1".to_string(),
            merged_node_id: "n1".to_string(),
            merge_phase: MergePhase::Planned,
            dependents: vec![],
        };
        write_stack_op_journal(tmp.path(), &journal).unwrap();
    }

    #[test]
    fn merge_phase_serde_round_trip() {
        let phase = MergePhase::PrMerged { sha: "abc123".to_string() };
        let json = serde_json::to_string(&phase).expect("MergePhase should serialize");
        let back: MergePhase = serde_json::from_str(&json).expect("MergePhase should deserialize");
        assert_eq!(back, phase);
    }

    #[test]
    #[should_panic(expected = "recover_in_flight_stack_op")]
    fn recover_in_flight_stack_op_panics_unimplemented() {
        let tmp = tempfile::tempdir().unwrap();
        recover_in_flight_stack_op(tmp.path()).unwrap();
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
