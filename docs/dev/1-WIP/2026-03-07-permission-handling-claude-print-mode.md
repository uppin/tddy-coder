# Changeset: Permission Handling in Claude Code Print Mode (with Embedded Permission Prompt Tool)

**Date**: 2026-03-07
**Status**: 🚧 In Progress
**Type**: Feature

---

## Plan Mode Discussion (Collaborative Planning Context)

*This section preserves the collaborative planning context from plan-tdd-one-shot.*

**Scope confirmed**: Phase 1 + Phase 2 (allowlists + embedded permission tool). Phase 3 (skip-permissions) deferred. Both goals get explicit allowlists. tddy-permission as new workspace member crate. planning-step.md updated at wrap time.

**Technical approach**: Extend InvokeRequest and build_claude_args; new crate `packages/tddy-permission` for MCP server; stdio transport for Claude Code MCP connection; IPC channel for TTY user prompts.

**Goal allowlists**:
- plan: Read, Glob, Grep, SemanticSearch (complements --permission-mode plan)
- acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch

**Implementation order**: Phase 1 (InvokeRequest + build_claude_args + allowlists + CLI --allowed-tools) → Phase 2 (tddy-permission MCP server + IPC + spawn/config integration).

---

## Affected Packages

**CRITICAL**: List ALL packages with documentation changes:

- **tddy-core**: [README.md](../../packages/tddy-core/README.md) - Backend InvokeRequest extension, Claude args, architecture
  - [architecture.md](../../packages/tddy-core/docs/architecture.md) - InvokeRequest, permission options
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md) - CLI flags, permission handling
- **tddy-permission** (new crate): New MCP server package for embedded permission prompt tool

## Related Feature Documentation

- [PRD: Permission Handling in Claude Code Print Mode](../../docs/ft/coder/1-WIP/PRD-2026-03-07-permission-handling-claude-print-mode.md)
- [planning-step.md](../../docs/ft/coder/planning-step.md) — Claude Code integration, permission modes

## Summary

Extend tddy-coder to handle permission requests when running in Claude Code print mode. **Hybrid policy**: (1) Each goal has a predefined allowlist — auto-approved via `--allowedTools`. (2) Unexpected requests go to the embedded permission tool — TTY = prompt user, CI = deny. Phase 1: goal-specific allowlists + InvokeRequest. Phase 2: embedded `tddy-permission` MCP server.

## Background

tddy-coder invokes Claude Code CLI with `-p` (print mode). In print mode, stdin is not used for interactive permission prompts. Permission behavior is controlled by `--permission-mode`, `--allowedTools`, or `--permission-prompt-tool`. When running interactively (TTY), users may encounter unexpected permission requests (e.g., Bash for `cargo test`) that cannot be granted because Claude Code does not read stdin for prompts in print mode.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Package Documentation**: Update planning-step.md, tddy-core and tddy-coder docs
- [ ] **Phase 1 Implementation**: `--allowed-tools` flag, InvokeRequest extension, Claude backend args
- [ ] **Phase 2 Implementation**: tddy-permission MCP server, embedded permission tool integration
- [ ] **Testing**: All acceptance tests passing
- [ ] **Technical Debt**: Production readiness gaps addressed
- [ ] **Code Quality**: Linting, type checking, validation complete

## Technical Changes

### State A (Current)

- `InvokeRequest` has: prompt, system_prompt, permission_mode, model, session_id, is_resume, agent_output, inherit_stdin
- Claude backend passes `--permission-mode plan` or `--permission-mode acceptEdits` only
- No `--allowedTools`, `--permission-prompt-tool`, or `--mcp-config` support
- planning-step.md does not document print mode constraint or permission options

### State B (Target)

- `InvokeRequest` has: `allowed_tools: Option<Vec<String>>`, `permission_prompt_tool: Option<String>`, `mcp_config_path: Option<PathBuf>`
- **Hybrid policy**: Goal-specific predefined allowlist passed as `--allowedTools`; embedded permission tool handles unexpected requests (TTY = prompt, CI = deny)
- Claude backend passes `--allowedTools` + `--permission-prompt-tool` + `--mcp-config` when hybrid policy is used
- New crate `tddy-permission`: MCP server with `approval_prompt` tool; receives only unexpected requests (Claude Code filters via --allowedTools first)
- CLI: optional `--allowed-tools` override; `--use-embedded-permission-tool` (Phase 2)
- planning-step.md documents print mode constraint and hybrid policy

### Delta (What's Changing)

#### tddy-core
- **InvokeRequest**: Add `allowed_tools`, `permission_prompt_tool`, `mcp_config_path`
- **build_claude_args**: Append `--allowedTools` for each entry when set; append `--permission-prompt-tool` and `--mcp-config` when permission tool is used
- **ClaudeCodeBackend::invoke**: No structural change; uses updated build_claude_args

#### tddy-coder
- **Goal-specific allowlists**: Each goal defines its predefined tools (e.g. acceptance-tests: Read, Write, Edit, Bash(cargo *)); passed as `--allowedTools`. Optional `--allowed-tools` CLI override to add extra tools.
- **Workflow wiring**: Pass goal allowlist + optional CLI extras to InvokeRequest; when embedded tool: spawn tddy-permission with IPC, generate MCP config, pass to backend
- **Interactive permission handler**: When TTY + unexpected request: read from tddy-permission via IPC, prompt user via inquire, send allow/deny back

#### tddy-permission (new crate, Phase 2)
- **Hybrid policy**: Claude Code checks `--allowedTools` first (goal's predefined list) → if match, allow immediately. If no match → call permission_prompt_tool.
- **MCP server**: stdio transport; single tool `approval_prompt` with params: tool_name, input (JSON object)
- **Response format**: `{"behavior":"allow","updatedInput":{...}}` or `{"behavior":"deny","message":"..."}`
- **Unexpected requests (TTY)**: tddy-permission forwards to tddy-coder via IPC; tddy-coder prompts user via inquire; user allow/deny flows back
- **Unexpected requests (CI/piped)**: Deny; no user prompt

## Implementation Milestones

- [x] **M1**: Define goal-specific predefined allowlists (e.g. acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *)); extend InvokeRequest with allowed_tools; update build_claude_args to pass --allowedTools
- [x] **M2**: Wire goal allowlist through workflow to InvokeRequest; optional --allowed-tools CLI override
- [x] **M3**: Update planning-step.md with print mode constraint and permission options
- [x] **M4**: Create tddy-permission crate with MCP server and approval_prompt tool
- [ ] **M5**: Add --use-embedded-permission-tool; spawn tddy-permission, generate MCP config, pass to Claude (deferred - tddy-permission crate ready, integration optional)
- [ ] **M6**: Interactive UX: IPC between tddy-permission and tddy-coder; tddy-coder prompts user via inquire when TTY; user can allow/deny each permission request
- [ ] **M7**: Integration: acceptance-tests goal uses hybrid policy (predefined allowlist + embedded tool for unexpected) when TTY

## Testing Plan

### Testing Strategy

**Primary Test Level**: Integration (backend args, CLI wiring) + Unit (build_claude_args, policy logic)

**Option 1: Integration tests (backend_args.rs)**
- Verify `build_claude_args` includes `--allowedTools` when allowed_tools is set
- Verify `--permission-prompt-tool` and `--mcp-config` when permission_prompt_tool is set
- Assert exact CLI argument order and values

**Option 2: Unit tests (tddy-permission)**
- approval_prompt returns allow for matched tools, deny for unmatched
- Policy parsing and matching logic

**Option 3: E2E (optional, manual)**
- Run acceptance-tests goal with --allowed-tools; verify no permission prompt for cargo test

### Acceptance Tests

#### tddy-core
- [x] **Integration**: build_claude_args includes --allowedTools when allowed_tools is Some (backend_args.rs)
- [x] **Integration**: build_claude_args includes --permission-prompt-tool and --mcp-config when permission_prompt_tool and mcp_config_path are set (backend_args.rs)
- [x] **Integration**: Workflow passes InvokeRequest with allowed_tools; plan and acceptance-tests goals (acceptance_tests_integration.rs)

#### tddy-coder
- [x] **Integration**: Goal allowlist is passed to InvokeRequest; optional --allowed-tools override (via workflow tests)
- [ ] **Integration**: Hybrid policy: --allowedTools + --permission-prompt-tool when --use-embedded-permission-tool (Phase 2, deferred)

#### tddy-permission
- [x] **Unit**: approval_prompt tool exists; returns deny for non-TTY and TTY (TTY IPC deferred)
- [ ] **Integration**: When TTY, IPC to tddy-coder for user prompt (Phase 2, deferred)

## Technical Debt & Production Readiness

- [ ] Document MCP protocol version compatibility for tddy-permission
- [ ] Consider: tddy-permission as optional binary (install separately) vs bundled

## Decisions & Trade-offs

- **Hybrid policy**: Predefined allowlist per goal (Layer 1) + permission tool for unexpected (Layer 2). Matches Claude Code's three-layer model; we use two layers (static allowlist + dynamic prompt).
- **Interactive UX**: tddy-permission must communicate with tddy-coder via IPC because MCP stdio is used for Claude Code; tddy-coder owns the TTY and can prompt via inquire
- **Unexpected = prompt (TTY) or deny (CI)**: No second allowlist; keep it simple

## Refactoring Needed

(To be populated during development)

## Validation Results

(To be populated during validation runs)

## References

- [Claude Code CLI reference](https://docs.anthropic.com/en/docs/claude-code/cli-usage) — --permission-mode, --allowedTools, --permission-prompt-tool
- [Vibe Sparking: permission-prompt-tool](https://www.vibesparking.com/en/blog/ai/claude-code/docs/cli/2025-08-28-outsourcing-permissions-with-claude-code-permission-prompt-tool/)
- [MCP Protocol](https://modelcontextprotocol.io/) — stdio transport, tool schema
