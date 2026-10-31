# tddy-coding-agent-specs

Coding agents as declared tools (`#agent-tools`). Each agent is a `tddy-actions` `ToolDef` whose
`run:` section is its argv template and whose `extensions.coding_agent` section declares the tddy
**mapping slots**: where the MCP server registration goes, the allow/deny and permission-prompt
flags, the instruction files a preamble is prepended to, the hooks adapter, the models and the
context globs.

- `specs/claude.yaml` — Claude Code CLI, embedded; overridable whole by `<tddyhome>/tools/claude.yaml`.
- `CodingAgentExtension` — the `ExtensionOwner` for `coding_agent`.
- `CodingAgentSpec::of(&ToolDef)` — the validated view of one agent.
- `slots::render` — the MCP file, flag argv, instruction files and hooks file for one launch.

PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md`.
