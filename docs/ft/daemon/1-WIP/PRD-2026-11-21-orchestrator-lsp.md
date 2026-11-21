# An orchestrator queries the LSP of any tddy worktree and adds or removes roots - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 8/9 — branch `feature/orchestra/orchestrator-lsp`, base `feature/orchestra/orchestrator-delivery`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

Orchestrator-role sessions get `LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover` and `LspSymbols` with a `root` argument naming any worktree in their reach (n1) — another session's worktree, another tddy project. `AddLspRoot` warms the index for a root; a root outside reach goes through the same operator request as a path (n6). `RemoveLspRoot` unloads a root this session added, without asking. `ListLspRoots` shows what the session added.

## Background

LSP tools are set only on sandboxed CLI-harness starts (`jail_env_builders.rs:62-84`); the role guard refuses them for orchestrators; the root is always the caller's own worktree; the index daemon has no unload RPC.

## Proposed Changes

### What's Changing
- `Lsp*` accept `root`; omitted means the caller's home worktree.
- `AddLspRoot { root, reason }`: in reach → warm at once; outside → `RequestPathAccess`-style request in the pane, warmed on Allow.
- `RemoveLspRoot { root }`: allowed for roots this session added; others refused.
- Index daemon `UnloadRoot` stops a root's language server.

### What's Staying the Same
- Worker sessions' LSP behaviour.
- Idle reaping.

## Impact Analysis

### Technical Impact
- **tddy-lsp-executor**: `Lsp*` tools take an optional `root`; `IndexLspExecutor` binds to it
- **tddy-index-daemon**: `UnloadRoot` RPC (today roots are only reaped when idle)
- **tddy-tool-engine**: `Lsp*` in the orchestrator catalogue; `AddLspRoot`, `RemoveLspRoot`, `ListLspRoots`
- **tddy-daemon-rpc**: role guard admits the tools; root added outside reach goes through n6's request flow
- **tddy-agent-launch**: orchestrator wiring offers the LSP tools

### User Impact
An orchestrator can navigate and check code in any session or project the way a coding session does.

## Implementation Plan

One node: `feature/orchestra/orchestrator-lsp` (#orchestra 8/9).

## Acceptance Criteria

- [ ] An orchestrator asks for references in a sibling session's worktree.
- [ ] Adding a root inside reach warms it without a request.
- [ ] Adding a root outside reach raises a request; Allow warms it.
- [ ] Removing a root this session added unloads it.
- [ ] Removing a root the session did not add is refused.
