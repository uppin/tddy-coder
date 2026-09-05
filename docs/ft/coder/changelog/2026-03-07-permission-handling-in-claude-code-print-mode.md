# 2026-03-07 — Permission Handling in Claude Code Print Mode

- **Print mode constraint**: tddy-coder uses Claude Code in print mode (`-p`); stdin is not used for interactive permission prompts
- **Hybrid policy**: Each goal has a predefined allowlist passed as `--allowedTools`; plan: Read, Glob, Grep, SemanticSearch; acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch
- **CLI**: `--allowed-tools` adds extra tools to the goal allowlist; `--debug` prints Claude CLI command and cwd
- **tddy-permission crate**: MCP server with `approval_prompt` tool for unexpected permission requests (TTY IPC deferred)
