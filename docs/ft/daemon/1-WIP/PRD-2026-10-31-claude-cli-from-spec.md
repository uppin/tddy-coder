# Claude Code CLI launches from its tool definition - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 2/12` (branch `feature/agent-tools/claude-host`)

## Affected Features

- **Primary**: [Claude CLI session](../claude-cli-session.md). An unsandboxed `claude-cli` session's binary, argv, env and tddy integration come from `claude.yaml`.
- **Related**: [Sandbox builder](../../coder/sandbox-builder.md). `ToolDef` gains a generic `run:` section (argv template, env, transport) and open `extensions:`.
- **Related**: [Session agent roster](../session-agent-roster.md). Unchanged here; the agents it lists keep their ids.

## Summary

A new crate, **`tddy-coding-agent-specs`**, holds the coding-agent tool definitions. They are embedded in the binary and overridable through `<tddyhome>/tools/<id>.yaml`. Its first agent is **`claude.yaml`**. An unsandboxed `claude-cli` session (start and resume) is launched from it through **one spec-driven PTY launcher**. Nothing in that launch path hard-codes Claude.

The definition declares three things:

- **Generic `run:`** (on `ToolDef`, usable by any tool): an argv **template** with named parameters and optional segments (`{model}`, `{session_id}`, `{resume}`, `{permission_mode}`, `{prompt}`), static env, and `transport: pty`.
- **`deps: [tddy-tools]`**: `tddy-tools` becomes a built-in tool, which is how the MCP relay binary is found.
- **The `coding_agent` extension**, owned by `tddy-coding-agent-specs`, which declares the tddy **mapping slots**:
  - `mcp`: where the tool server registration goes and in what shape. Claude takes a JSON file passed by `--mcp-config`.
  - `tool_allow` and `tool_deny`: the argv flags for the allow-list and the withdrawn native tools (`--allowedTools` / `--disallowedTools`).
  - `permission_prompt`: `--permission-prompt-tool`.
  - `instructions`: the context files a preamble is prepended to (`CLAUDE.md`, `AGENTS.md`), and the append-system-prompt flag.
  - `hooks`: an adapter **name** (`claude-settings-local`). The hook protocol stays in Rust.
  - `models`: the curated list and the default. `context_globs`: what context sync copies.

```yaml
id: claude
binary: { command: claude, env: TDDY_CLAUDE_BINARY }
deps: [tddy-tools]
run:
  transport: pty
  argv: [ "{binary}", "--model", "{model}",
          { if: resume, then: ["--resume", "{session_id}"], else: ["--session-id", "{session_id}"] },
          { if: permission_mode, then: ["--permission-mode", "{permission_mode}"] },
          { if: prompt, then: ["{prompt}"] } ]
  env: { CLAUDE_CODE_NO_FLICKER: "1" }
extensions:
  coding_agent:
    mcp: { kind: file, flag: --mcp-config, format: claude-mcp-json }
    tool_allow: { flag: --allowedTools }
    tool_deny: { flag: --disallowedTools }
    permission_prompt: { flag: --permission-prompt-tool }
    instructions: { files: [CLAUDE.md, AGENTS.md], append_system_prompt: { flag: --append-system-prompt-file } }
    hooks: { adapter: claude-settings-local }
    models: { default: opus, curated: [opus, sonnet, haiku, ...] }
    context_globs: [CLAUDE.md, ".claude/**", .mcp.json]
```

## Background

A `claude-cli` launch today is assembled across several places:

- `claude_argv.rs` and `cli-sessions/argv.rs` build the argv.
- `agent_cli.rs` reads daemon.yaml `claude_cli:` and resolves the binary.
- `claude_hooks.rs` writes the hooks.
- `MANAGED_CODEBASE_PREAMBLE` is handled in `tddy-sandbox`.
- Model tables live in `backend/mod.rs`.

Each copy encodes Claude by hand. Under `#agent-tools`, every agent is one definition, and the runtime fills its declared slots the same way wherever it runs. This node makes that true for Claude on the host, which is the simplest placement. That proves the template and the slots before the jail (`#agent-tools 5/12`) uses them.

## Proposed Changes

### What's Changing

- `ToolDef` gains `run:` (argv template, env, transport) and `extensions:`, a map from an owner name to a section the owner validates.
- An unknown extension owner is refused at load.
- `tddy-tools` becomes a built-in tool in `tddy-actions/tools/`.
- New crate `tddy-coding-agent-specs`:
  - `claude.yaml`;
  - the `CodingAgentSpec` view (`coding_agent(id)`), which validates the `coding_agent` extension;
  - the slot renderers, which produce the MCP file, the allow/deny argv and the instruction files from a launch's tool set.
- A spec-driven host PTY launcher in `tddy-agent-launch`. The unsandboxed `claude-cli` start and resume go through it.
- daemon.yaml `claude_cli.binary_path` and `TDDY_CLAUDE_BINARY` keep working as the binary override, because the spec's `binary.env` names that variable. Removal of the daemon.yaml key is `#agent-tools 12/12`.

### What's Staying the Same

- **Claude's observed behaviour.** Its argv, env, MCP config, hooks and preamble are the same, byte for byte where tests pin them.
- **The jailed `claude-cli` path** (`#agent-tools 5/12`), headless Claude (`6/12`), split sessions and `tddy-sandbox-app` (`10/12`).
- **Session types, RPCs and the `session_type: claude-cli` string.**

## Acceptance Criteria

- [ ] An unsandboxed `claude-cli` start runs the binary `claude.yaml` resolves, with the argv its template renders for model, session id, permission mode and prompt.
- [ ] Resume renders `--resume <id>` from the same template.
- [ ] The MCP config file, `--allowedTools`/`--disallowedTools`, `--permission-prompt-tool` and the managed preamble in `CLAUDE.md`/`AGENTS.md` are produced from the declared slots, and are equal to today's output for the same session.
- [ ] `<tddyhome>/tools/claude.yaml` replaces the built-in. An extra argv token there reaches the spawned process.
- [ ] The launcher refuses a spec whose `run.transport` is not `pty`, naming the spec.
- [ ] A spec with an unknown slot kind or extension owner is refused at load.

## Out of Scope

- The jail (`5/12`), egress (`3/12`), headless (`6/12`) and other agents (`7/12` onwards).
- Removing daemon.yaml `claude_cli:` (`12/12`).
