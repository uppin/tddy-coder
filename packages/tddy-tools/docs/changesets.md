# Changesets Applied

Wrapped changeset history for tddy-tools.

- **2026-03-13** [Bug Fix] Session and Workflow Fixes — Permission routing via TDDY_SOCKET, tool_in_repo_pre_allowed, non-blocking relay. path.is_some_and(Self::path_allowed). (tddy-tools)
- **2026-03-10** [Feature] tddy-tools Relay Handler — Renamed from tddy-permission. CLI with `submit`, `ask` subcommands and `--mcp` mode. Relays agent tool calls to tddy-coder presenter via Unix domain socket IPC. Local JSON schema validation before relay. Blocking ask for clarification questions. (tddy-tools)
- **2026-03-07** [Feature] Permission Handling in Claude Code Print Mode — New MCP server crate with approval_prompt tool. stdio transport. Denies unexpected requests (TTY IPC deferred). Used by Claude Code --permission-prompt-tool. (tddy-permission)
