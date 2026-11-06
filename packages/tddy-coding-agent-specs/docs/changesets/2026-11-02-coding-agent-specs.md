# 2026-11-02 — New crate: coding agents as declared tools

**Type:** Feature

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)).
- `specs/claude.yaml` is embedded.
- `CodingAgentExtension` owns the `coding_agent` extension, and `CodingAgentSpec::of` gives the validated view of one agent.
- `slots::render` fills the MCP file (with `strict_flag`), the allow/deny/permission-prompt flags (`ToolSlot = Flag | unsupported`), the instruction files and the hooks adapter (`claude-settings-local`).
- `load_agent_registry` builds the registry a host that launches coding agents uses.

See [coding-agent-specs.md](../coding-agent-specs.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
