//! Remote-codebase mode constants and helpers.
//!
//! When `--remote` is passed to tddy-coder, the agent operates against a remote worktree
//! via the `mcp__tddy-tools__*` MCP tools instead of native Claude Code filesystem tools.

/// Appended to CLAUDE.md and AGENTS.md in the remote context dir, and to the agent system prompt.
pub const REMOTE_APPENDIX: &str = r#"

## Appendix: Remote Codebase

The real codebase is REMOTE — it is NOT in this local directory.
This local directory is read-only and contains only documentation and synced skills.

You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations.
Do not use native tools to interact with the codebase.
"#;
