# PRD: Permission Handling in Claude Code Print Mode

**Product Area**: Coder
**Status**: Draft
**Created**: 2026-03-07
**PRD Type**: Technical Improvement

## Affected Features

**CRITICAL**: List ALL feature documents affected by this PRD:

- **Primary Feature**: [planning-step.md](../planning-step.md) — Claude Code integration, permission modes, and interactive session behavior

## Summary

tddy-coder invokes Claude Code CLI with `-p` (print mode) for non-interactive, single-query execution. In print mode, **stdin is not used for interactive permission prompts** — Claude Code handles permissions via `--permission-mode`, `--allowedTools`, or `--permission-prompt-tool`, not by reading user input from stdin. This PRD documents the current behavior, the constraint, and proposes options for handling unexpected permission requests when the user runs tddy-coder interactively (TTY).

## Background

### Print Mode Constraint

- **Print mode (`-p`)**: tddy-coder always uses `-p` for automation-friendly, single-query execution. Claude Code runs one command and exits.
- **Stdin in print mode**: Stdin can be used for piped prompt input (`cat file | claude -p "query"`), but tddy-coder passes the prompt as a CLI argument. Stdin is not consumed for the prompt.
- **Permission prompts**: In non-interactive (print) mode, Claude Code does not read stdin for interactive "Allow/Deny" permission prompts. Permission behavior is controlled by:
  - `--permission-mode` (plan, acceptEdits, etc.) — pre-approve behavior
  - `--permission-prompt-tool` — MCP tool for custom handling in non-interactive mode
  - `--allowedTools` — auto-allow specific tools via pattern matching

### Current tddy-coder Behavior

| Goal              | Permission Mode   | Behavior                                      |
|-------------------|-------------------|-----------------------------------------------|
| plan              | `--permission-mode plan` | Read-only; no file edits or Bash |
| acceptance-tests  | `--permission-mode acceptEdits` | Auto-approves file edits; Bash may still prompt |

### User Need

When running tddy-coder interactively (TTY), the user may encounter unexpected permission requests (e.g., Bash for `cargo test`). Currently, `inherit_stdin` passes the terminal stdin to the subprocess, but **Claude Code in print mode does not read stdin for permission prompts**. The user cannot grant such requests interactively.

## Proposed Changes

### Option A: `--allowedTools` (Recommended for acceptance-tests)

Add support for `--allowedTools` to pre-approve specific tools when running interactively:

- **CLI**: `--allowed-tools "Bash(cargo *)" "Bash(cargo test *)"` or similar
- **Use case**: Acceptance-tests goal runs `cargo test`; pre-approve Bash for cargo commands
- **Safety**: Explicit allowlist; user chooses what to pre-approve

### Option B: `--permission-prompt-tool` (MCP)

Integrate an MCP tool that handles permission prompts in non-interactive mode:

- **CLI**: `--permission-prompt-tool <mcp_tool_name>`
- **Use case**: Custom logic (e.g., auto-approve in CI, prompt in TTY)
- **Complexity**: Requires MCP server implementation

### Option C: `--dangerously-skip-permissions` (Trusted workflows)

Allow bypassing all permission prompts when the user explicitly opts in:

- **CLI**: `--dangerously-skip-permissions` (or `--allow-dangerously-skip-permissions` + mode)
- **Use case**: Fully trusted local development, CI pipelines
- **Safety**: User must explicitly enable; documented as dangerous

### What's Staying the Same

- Plan goal continues to use `--permission-mode plan` (read-only)
- Acceptance-tests goal continues to use `--permission-mode acceptEdits` for file edits
- `inherit_stdin` remains for potential future use (e.g., if Claude Code adds stdin-based permission prompts in print mode)

## Impact Analysis

### Technical Impact

- **Backend**: Extend `InvokeRequest` to support `allowed_tools: Option<Vec<String>>`, `permission_prompt_tool: Option<String>`, or `skip_permissions: bool`
- **Claude backend**: Pass `--allowedTools`, `--permission-prompt-tool`, or `--dangerously-skip-permissions` to the CLI when set
- **CLI**: Add corresponding flags; wire through to workflow invocations

### User Impact

- **Interactive sessions**: User can pre-approve tools (Option A) or skip prompts (Option C) when running from a TTY
- **CI/piped**: No change; stdin is not a TTY, so `inherit_stdin` is false; explicit flags would still apply if set

## Implementation Plan

1. **Phase 1 (Option A)**: Add `--allowed-tools` flag and `allowed_tools` on `InvokeRequest`; pass to Claude Code via `--allowedTools`
2. **Phase 2 (optional)**: Add `--dangerously-skip-permissions` for trusted workflows
3. **Phase 3 (optional)**: Add `--permission-prompt-tool` for MCP-based handling

## Acceptance Criteria

- [ ] planning-step.md documents: tddy-coder uses Claude Code in print mode; stdin is not used for permission prompts
- [ ] planning-step.md documents: permission handling options (`--allowedTools`, `--permission-prompt-tool`, `--dangerously-skip-permissions`)
- [ ] **Option A**: `--allowed-tools` flag allows pre-approving tools (e.g., `Bash(cargo *)`) for acceptance-tests goal
- [ ] **Option A**: `InvokeRequest` supports `allowed_tools`; Claude backend passes `--allowedTools` when set
- [ ] Tests verify that `allowed_tools` is passed correctly to the Claude CLI

## References

### Affected Features (Complete List)

- [planning-step.md](../planning-step.md) — Claude Code integration, permission modes, interactive session behavior

### Related Documentation

- [Claude Code CLI reference](https://docs.anthropic.com/en/docs/claude-code/cli-usage) — `--permission-mode`, `--allowedTools`, `--permission-prompt-tool`
- [Claude Code print mode](https://claudelog.com/faqs/what-is-print-flag-in-claude-code) — Non-interactive behavior
