//! Claude Code's own files: the transcript and sub-agent usage its sessions leave behind.
//!
//! A headless `claude -p` workflow turn is run by [`crate::SpecHeadlessBackend`] from `claude.yaml`;
//! nothing here runs a turn.

mod transcript_usage;
pub use transcript_usage::{read_claude_subagent_usages, read_claude_transcript_usage};
