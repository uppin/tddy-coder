# Headless `-p` coding agents as subagents - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 7/8, branch `feature/coagents/headless-driver`, base `feature/coagents/remote-codebase` (#725)

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — coding subagents over headless CLIs
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — headless harnesses listed as coding agents
- **Related Feature**: [Harness profiles](../../daemon/harness-profiles.md) — `claude`, `cursor`, `codex` profiles become attachable

## Summary

The ACP path covers every coding agent with an ACP adapter. The rest are driven headless: `claude -p
--output-format stream-json`, `cursor agent -p --output-format stream-json`, and `codex exec --json`.
This PRD puts a **headless transport behind the same conversation interface**, so those harnesses are
listed, attached and talked to exactly like ACP ones — same tools, same outcome shape, same activity
bubble, same placement — with ACP preferred whenever a harness has both.

## Background

The workflow backends for these CLIs exist (`claude.rs`, `cursor.rs`, `codex.rs`) as one-shot
`CodingBackend`s. They already resume (`--resume`, `exec resume`) and stream (claude and cursor);
codex reports nothing until it exits. Claude can route its permission prompts to an MCP tool
(`--permission-prompt-tool`); cursor runs with `--force`; codex runs in a sandbox mode.

## Proposed Changes

### What's Changing

- **One interface, two transports.** A conversation turn on a headless harness is one CLI invocation
  resuming the conversation's own session id; its streamed events become the same tool-call and text
  updates an ACP turn produces.
- **Which harnesses.** `claude`, `cursor`, `codex` harness profiles and `allowed_agents` ids are listed
  by `ListSubagents` as `coding_agent` rows. Where one agent is offered both ways (`claude` and
  `claude-acp`), both rows are listed, and each says its transport.
- **Permissions.**
  - `claude -p`: permission prompts are routed to `tddy-tools`, and yield to the main agent exactly
    as an ACP permission request does (`permission_requested` → `subagent_resume { permission }`).
  - `cursor` and `codex`: they cannot ask mid-turn. They are offered with a fixed policy named on the
    row — `cursor` read-only unless the operator's profile opts into `--force`, `codex` in its
    `workspace-write` sandbox — and the policy is shown in the picker.
- **Cancel** ends the CLI process; the turn returns `cancelled`.
- **Managed confinement** as for ACP: native tools withdrawn where the CLI allows it
  (`claude --disallowedTools`, `cursor`'s MCP-only mode if available), refused at attach otherwise.
- **Codex progress.** Codex turns report their tool calls when the turn ends (the CLI streams nothing
  sooner); the bubble shows "running" meanwhile.

### What's Staying the Same

- The workflow backends' behaviour and flags.
- ACP harnesses and their transport.
- Muse: still not offered (no headless or ACP mode).

## Impact Analysis

### Technical Impact

- `tddy-agent-backend`: a `HeadlessDriver` beside `AcpDriver`, reusing the stream parsers and argv
  builders of the three CLI backends.
- `tddy-coding-subagent`: `CodingAgentSession` over either driver.
- Catalog rows and confinement table extended.

### User Impact

- Every coding agent that tddy can run headless can be a subagent.

## Acceptance Criteria

- [ ] `claude`, `cursor`, `codex` are listed as coding agents with their transport and permission policy.
- [ ] A turn on each returns the CLI's final text, and a second turn resumes the same CLI session.
- [ ] Claude's permission prompt yields to the main agent and its answer is honoured.
- [ ] Cancel ends a running headless turn as `cancelled`.
- [ ] Tool calls stream into the conversation transcript (codex at turn end).
- [ ] A headless harness that cannot be confined is refused in managed sessions, naming it.

## References

- [Managed-codebase subagents](../managed-codebase-subagents.md)
- Stack parents: #720–#725
