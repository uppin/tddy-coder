//! The journal the daemon writes beside a queue before each destructive step of a land
//! (`tddy-daemon-rpc` `git_work::land_ops`), so an interrupted merge is visible on disk.

/// Phase of the merge+repoint atomic operation for one node.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "phase")]
pub enum MergePhase {
    Planned,
    PrMerged { sha: String },
    Done,
}

/// Journal written to disk before each destructive step, enabling crash recovery.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkQueueOpJournal {
    pub op_id: String,
    pub merged_node_id: String,
    pub merge_phase: MergePhase,
    pub dependents: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_phase_serde_round_trip() {
        let phase = MergePhase::PrMerged {
            sha: "abc123".to_string(),
        };
        let json = serde_json::to_string(&phase).expect("MergePhase should serialize");
        let back: MergePhase = serde_json::from_str(&json).expect("MergePhase should deserialize");
        assert_eq!(back, phase);
    }
}
