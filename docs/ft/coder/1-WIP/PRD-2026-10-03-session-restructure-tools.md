# Restructure tool calls in sessions - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Rust code restructuring](../rust-code-restructuring.md) — restructure as agent tool calls.
- **Related**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md).

## Summary

An agent can restructure only by shelling out to `tddy-tools restructure`, which needs
`TDDY_INDEX_SOCKET` — set in no session, and unreachable from inside a jail. This PRD adds tool calls
`restructure_load`, `restructure_check`, `restructure_apply`, `restructure_status`,
`restructure_plans`, `restructure_anchors`, relayed to the host like the `Lsp*` tools and executed
against the daemon-managed warm index on the session's own worktree, with structured JSON results.

## Proposed Changes

- A new tool module in `tddy-tools` (not `server.rs`, which is over budget), gated by
  `TDDY_RESTRUCTURE_TOOLS` set by the host when it can serve them (the `TDDY_LSP_TOOLS` precedent).
- Host: exec-tool handlers dial `IndexDaemonRegistry::connect`, bind the session's worktree root
  host-side, resolve plan paths inside it (refusing anything outside), and map `RestructureEvent`s to
  a JSON result (findings, per-op outcome, refusal class).
- The tool names join `IN_JAIL_RELAYABLE_EXEC_TOOLS`; the advertisement audit is updated.

## Acceptance Criteria
- [x] `restructure_check` on a plan in the worktree returns its findings as JSON.
- [x] `restructure_apply` applies through the warm index and returns per-op outcomes; a stale op is refused by id.
- [x] A plan path outside the session's worktree is refused host-side.
- [ ] From inside a jail the tools reach the host; without `index_daemon:` they are not advertised.
