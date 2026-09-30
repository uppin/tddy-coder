//! Which exec tools can change a worktree.
//!
//! **Fail-closed**: a tool is read-only only if it is named here. Everything else — `Await`
//! included, because a background `Shell` job keeps changing files while it is awaited, and any name
//! this list has never heard of — is [`ToolEffect::Mutating`], and a mutating call is followed by a
//! commit. A wrongly-mutating read costs one empty commit check; a wrongly-read-only write would
//! leave a change no commit accounts for.

/// What a tool call can do to the worktree it runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolEffect {
    /// `Read`, `Glob`, `Grep`, `SemanticSearch`, `ReadLints` — never changes a file.
    ReadOnly,
    /// Everything else.
    Mutating,
}

impl ToolEffect {
    /// Classify an exec-catalog tool name (`"Write"`, `"StrReplace"`, …).
    pub fn of(tool_name: &str) -> Self {
        // TODO(isolated-edits): implement
        todo!("ToolEffect::of({tool_name:?})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_reading_tools_are_read_only() {
        let effects: Vec<ToolEffect> = ["Read", "Glob", "Grep", "SemanticSearch", "ReadLints"]
            .into_iter()
            .map(ToolEffect::of)
            .collect();
        assert_eq!(effects, vec![ToolEffect::ReadOnly; 5]);
    }

    #[test]
    fn the_writing_tools_are_mutating() {
        let effects: Vec<ToolEffect> = ["Write", "StrReplace", "Delete", "Shell"]
            .into_iter()
            .map(ToolEffect::of)
            .collect();
        assert_eq!(effects, vec![ToolEffect::Mutating; 4]);
    }

    #[test]
    fn await_is_mutating_because_a_background_job_writes_while_awaited() {
        assert_eq!(ToolEffect::of("Await"), ToolEffect::Mutating);
    }

    #[test]
    fn an_unknown_tool_is_mutating() {
        assert_eq!(ToolEffect::of("LspRename"), ToolEffect::Mutating);
    }

    #[test]
    fn the_match_is_case_sensitive_so_a_misspelt_read_is_mutating() {
        assert_eq!(ToolEffect::of("read"), ToolEffect::Mutating);
    }
}
