# Session LSP tools answered by the warm index - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — sessions become its second consumer.
- **Related**: [Reusable LSP](../reusable-lsp.md) — the agent `Lsp*` tools stop starting a host-side server of their own for a Rust worktree when the daemon manages an index.

## Summary

An agent's `LspDefinition`, `LspReferences`, `LspHover`, `LspSymbols` and `LspDiagnostics` tool
calls are executed on the host by `tddy_lsp_executor`, which runs its own rust-analyzer per
`BUILD.yaml` target. This PRD answers them from the warm index tddy-daemon manages, through the
navigation RPCs `code-navigation` added, so a session asks the same index the web does and pays no
second cold load.

## Proposed Changes

- Host side: when the daemon has an `index_daemon:` section, the `Lsp*` exec tools resolve against
  `IndexDaemonRegistry::connect` + the session's **own worktree**, bound host-side from the session
  (never from the jail's arguments — see
  [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../../../dev/todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)).
- `LspSymbols` and `LspDiagnostics` need two more index RPCs (`Symbols`, `Diagnostics`), added here.
- Without `index_daemon:` the tools keep today's executor — the deployment switch, not a fallback
  within one deployment.
- Tool names, argument schemas and result JSON are unchanged, so the advertised set pinned by
  `mcp_tool_advertisement_audit.rs` does not move.

## Acceptance Criteria
- [ ] With `index_daemon:` configured, `LspDefinition` from a session returns the same location the
      code pane does, and no `tddy_lsp_executor` server is started.
- [ ] A query naming a path outside the session's worktree is refused host-side.
- [ ] `LspSymbols` and `LspDiagnostics` are answered by the index.
- [ ] Without `index_daemon:` the tools behave exactly as today.
- [ ] Tool names and result shapes are unchanged.
