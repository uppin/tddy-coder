# Coding agent specs (`tddy-coding-agent-specs`)

Product contract: [Claude CLI session § Launch](../../../docs/ft/daemon/claude-cli-session.md#launch-from-the-claude-tool-definition).
Tool schema: [`tddy-actions` declared tools](../../tddy-actions/docs/declared-tools.md).

A coding agent is a `tddy_actions::tool::ToolDef`:

- its `run:` section is the argv template;
- its `deps` name the tools it calls back through (`tddy-tools`);
- its `extensions.coding_agent` section declares the tddy **mapping slots**.

This crate owns that section's vocabulary. `tddy-actions` stays agent-agnostic.

| Module | Holds |
|---|---|
| `lib.rs` | `builtin_specs()` (the embedded `specs/*.yaml`). `load_agent_registry(overrides_dir)`: `BUILTIN_TOOLS ++ BUILTIN_SUPPORT_TOOLS ++ builtin_specs()` through `ToolRegistry::from_embedded`, then `load_with_owners` with `<tddyhome>/tools` and `[CodingAgentExtension]` |
| `spec.rs` | `CodingAgentExtension`, the `ExtensionOwner` for `coding_agent`, which deserializes the section. `CodingAgentSpec::of(&ToolDef)` returns `{ id, run, agent: CodingAgentSection }`, or `SpecError::{NotACodingAgent, NoRunSection, Invalid}`. The section types are `McpSlot::File { flag, format: McpFormat::ClaudeMcpJson, strict_flag }`, `ToolSlot = Flag(FlagSlot) \| unsupported` for `tool_allow` / `tool_deny` / `permission_prompt`, `InstructionsSlot { files, append_system_prompt }`, `HooksSlot { adapter }`, `ModelsSection { default, curated }` and `context_globs` |
| `slots/mod.rs` | `SlotInputs` (workdir, scratch dir, MCP server, allowed / denied tools, permission-prompt tool, preamble, append-system-prompt file, hook params). `render(&CodingAgentSpec, &SlotInputs) -> RenderedSlots { argv_extra, files }`. `SlotError::{UnknownHooksAdapter, Io, SlotUnsupported}` |
| `slots/mcp.rs` | the registration file `<scratch>/claude-mcp-config.json` (`{"mcpServers": {<name>: {command, args, env?}}}`), and `flag <path> [strict_flag]` |
| `slots/tools.rs` | one flag per value (`--allowedTools a --allowedTools b`) |
| `slots/instructions.rs` | the preamble prepended once to each declared instruction file that exists. A file already led by it is unchanged |
| `slots/hooks.rs` | `HooksAdapter`, and `ClaudeSettingsLocal` (`claude-settings-local`), which renders `<workdir>/.claude/settings.local.json` with `tddy_agent_backend::build_claude_hooks_settings`. `adapter(name)` returns the adapter, and refuses an unknown name |
| `specs/claude.yaml` | Claude Code CLI |

## Rules

- **Nothing is written here.** The renderers return bytes and argv, and the launcher
  (`tddy_agent_launch::spec_launch::launch_host_pty`) writes `RenderedSlots::files` before it spawns.
  Only `instructions` reads the disk.
- **`argv_extra` is ordered** as the append-system-prompt flag, the allow flags, the deny flags,
  the permission-prompt flag, then the MCP flags. The template splices it before the positional
  prompt. The MCP `strict_flag` keeps the variadic `--mcp-config` from swallowing that prompt.
- **An `unsupported` slot with values is refused** (`SlotUnsupported`), never dropped. With nothing to
  pass, it renders nothing.
- **The hook protocol is not declarable.** A spec names an adapter, and the adapter is Rust.

Tests: `tests/spec_load_acceptance.rs` (the built-in spec, an unknown slot kind, an unknown extension
owner, and its positive control). Unit tests cover `slots::tests`, `slots::mcp::tests`,
`slots::hooks::tests` and `spec::tool_slot_tests`.
