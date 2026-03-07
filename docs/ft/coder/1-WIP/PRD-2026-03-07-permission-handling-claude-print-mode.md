# PRD: Permission Handling in Claude Code Print Mode

**Product Area**: Coder
**Status**: In Progress
**Created**: 2026-03-07
**PRD Type**: Technical Improvement
**Scope**: Phase 1 (allowlists) + Phase 2 (embedded permission tool). Phase 3 (skip-permissions) deferred.

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

**Decision**: Both goals will receive explicit allowlists (Phase 1). The plan goal's allowlist complements `--permission-mode plan` for extra safety.

### User Need

When running tddy-coder interactively (TTY), the user may encounter unexpected permission requests (e.g., Bash for `cargo test`). Currently, `inherit_stdin` passes the terminal stdin to the subprocess, but **Claude Code in print mode does not read stdin for permission prompts**. The user cannot grant such requests interactively.

## Proposed Changes

### Hybrid Policy (Option A + B)

Permission handling uses a **hybrid** two-layer model:

1. **Predefined allowlist per goal**: Each goal (plan, acceptance-tests, etc.) has a built-in list of tools expected during execution. These are passed as `--allowedTools` and auto-approved — no prompt.
2. **Unexpected permissions**: When a permission request does **not** match the predefined list, the embedded permission tool handles it:
   - **Interactive (TTY)**: Prompt the user (e.g. "Allow Bash(cargo test)? [y/n]"); user can allow or deny.
   - **Non-interactive (CI/piped)**: Deny by default; no user prompt.

**Example (acceptance-tests goal)**: Predefined allowlist might include `Read`, `Write`, `Edit`, `Bash(cargo *)`. If Claude requests `Bash(cargo test)` → allowed (matches). If Claude requests `Bash(npm install)` → unexpected → prompt user (TTY) or deny (CI).

### Embedded Permission Tool (Option B)

- **Embedded**: tddy-coder ships `tddy-permission` MCP server with `approval_prompt` tool. No external setup.
- **CLI**: `--permission-prompt-tool` (uses embedded) or `--permission-prompt-tool <mcp_tool_name>` (external)

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
- **New crate (Phase 2)**: `tddy-permission` — MCP server binary implementing `approval_prompt` tool; spawned by tddy-coder. When TTY: IPC to tddy-coder for user prompt (inquire); when non-TTY: policy-based allow/deny.

### User Impact

- **Interactive sessions (TTY)**: Predefined tools auto-allowed; **unexpected** requests prompt the user. User can allow or deny each unexpected request.
- **CI/piped**: Predefined tools auto-allowed; unexpected requests denied. No interactive prompt.

## Implementation Plan

1. **Phase 1** (In Scope): Define goal-specific predefined allowlists for both plan and acceptance-tests goals; add `allowed_tools` on `InvokeRequest`; pass to Claude Code via `--allowedTools`
2. **Phase 2** (In Scope): Create `tddy-permission` as new workspace member crate (`packages/tddy-permission`); MCP server with `approval_prompt` tool; handles unexpected requests (TTY = prompt user, CI = deny)
3. **Phase 3** (Deferred): `--dangerously-skip-permissions` for trusted workflows

## Acceptance Criteria

- [ ] planning-step.md documents: tddy-coder uses Claude Code in print mode; stdin is not used for permission prompts
- [ ] planning-step.md documents: hybrid policy (predefined allowlist per goal + user prompt for unexpected requests)
- [ ] **Hybrid policy**: Each goal has a predefined allowlist; passed as `--allowedTools`; unexpected requests go to permission tool
- [ ] **InvokeRequest** supports `allowed_tools`; Claude backend passes `--allowedTools` when set
- [ ] **Embedded tool**: tddy-coder ships `tddy-permission` MCP server with `approval_prompt` tool
- [ ] **Unexpected requests (TTY)**: Prompt the user; user can allow or deny
- [ ] **Unexpected requests (CI)**: Deny by default

## References

### Affected Features (Complete List)

- [planning-step.md](../planning-step.md) — Claude Code integration, permission modes, interactive session behavior

### Related Documentation

- [Claude Code CLI reference](https://docs.anthropic.com/en/docs/claude-code/cli-usage) — `--permission-mode`, `--allowedTools`, `--permission-prompt-tool`
- [Claude Code print mode](https://claudelog.com/faqs/what-is-print-flag-in-claude-code) — Non-interactive behavior
