# Code navigation in the session code explorer, served by the warm index - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Session code pane](../session-code-pane.md) — the read-only preview gains go-to-definition, references and hover.
- **Related**: [Warm code-intelligence daemon](../../coder/warm-code-intelligence-daemon.md) — `code_index.CodeIndexService` gains `Definition`, `References`, `Hover`; tddy-daemon becomes its first consumer.

## Summary

The code explorer shows a session's worktree as plain highlighted text. This PRD makes it navigable:
click an identifier to jump to its definition (opening the target file at that line), list its
references, and see its hover (type and docs). The answers come from the warm rust-analyzer index
that tddy-daemon already spawns lazily, reached through a new authorised daemon service — the first
production caller of `IndexDaemonRegistry::connect`.

## Background

- `WorktreeCodePane` renders `CodeBlock` (PrismLight) with no token positions; file content comes from `worktree.WorktreeService`.
- `CodeIndexService` has no navigation RPCs; `tddy-lsp`'s `LspClient` already implements `definition`/`references`/`hover`.
- `IndexDaemonRegistry` (lazy spawn, readiness, idle stop) has no consumer: [`2026-09-16-indexdaemonregistry-connect-has-no-caller.md`](../../../dev/todo/2026-09-16-indexdaemonregistry-connect-has-no-caller.md).
- No `code_index` TS bindings exist; nothing proxies the index to the web.

## Proposed Changes

1. **Index daemon:** `Definition`, `References`, `Hover` RPCs on `CodeIndexService` — `{workspace_root, file, position}` in the 1-based byte coordinates the service already uses; answered through `WorkspaceIndex::client_for(root)` and `LspClient`; locations returned relative to the root, outside-root locations marked.
2. **tddy-daemon:** a web-facing `code_navigation.CodeNavigationService` (proto in `tddy-service`, so the web generates it) with the same three calls keyed by `{session_token, project_id, worktree_path, rel_path, position}`, authorised exactly like `WorktreeService` (`resolve_listed_worktree`), forwarded via `IndexDaemonRegistry::connect`. Without an `index_daemon:` section the service answers `FailedPrecondition` naming the missing config — no fallback to another LSP.
3. **Web:** `CodeBlock` becomes position-aware: hover shows a popover; ctrl/cmd-click jumps to the definition (same pane, file + line scrolled into view); a "References" action lists locations; each is clickable.

### Staying the same
File reading, tree listing and their authorisation; the existing agent LSP tools (a successor moves them).

## Acceptance Criteria
- [ ] `Definition` on a call returns the callee's file and line within the worktree, in 1-based byte coordinates.
- [ ] `References` returns every reference, `Hover` the type text.
- [ ] The daemon refuses a worktree path not listed for the project (same codes as `WorktreeService`).
- [ ] With no `index_daemon:` config the service answers `FailedPrecondition` naming it.
- [ ] The first call starts the index daemon (lazy); the `connect` backlog entry is closed.
- [ ] In the code pane, ctrl-click on an identifier opens the definition file scrolled to its line; hover shows the type; the references list navigates.
