/// Whether `tool_name` is a native mutation tool that must be hard-denied when the agent is
/// running against a remote codebase (`TDDY_REMOTE_SESSION_ID` set).
///
/// A denial rather than an absence: in remote mode the local working directory is not the
/// worktree the agent is editing, so a native write would corrupt it silently. The agent needs
/// to be told it was refused, not that the tool was never there — the replacement is this
/// crate's own `Write`, dispatched against the real worktree.
pub fn is_native_tool_denied_in_remote_mode(tool_name: &str) -> bool {
    matches!(tool_name, "Write" | "Edit" | "NotebookEdit")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three tools a remote-mode agent must not reach are the three that write to disk.
    #[test]
    fn denies_every_native_tool_that_writes_to_the_local_disk() {
        // Given / When / Then
        for tool in ["Write", "Edit", "NotebookEdit"] {
            assert!(
                is_native_tool_denied_in_remote_mode(tool),
                "{tool} writes to disk and must be denied in remote mode"
            );
        }
    }

    /// The denial is a list, not a default: refusing a read or the approval prompt would take
    /// the agent's ability to ask for anything.
    #[test]
    fn leaves_reads_and_the_approval_prompt_alone() {
        // Given / When / Then
        for tool in [
            "Read",
            "Grep",
            "approval_prompt",
            "submit",
            "AskUserQuestion",
        ] {
            assert!(
                !is_native_tool_denied_in_remote_mode(tool),
                "{tool} does not write to the local disk and must not be denied"
            );
        }
    }
}
