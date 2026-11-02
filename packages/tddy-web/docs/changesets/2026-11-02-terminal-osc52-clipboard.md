# 2026-11-02 — A terminal's OSC 52 copy reaches the viewer's clipboard

**Type:** Feature

`#session-tabs 4/7` ([#807](https://github.com/uppin/tddy-coder/pull/807)). New `src/lib/osc52.ts` (a streaming OSC 52 write recognizer, 1 MiB cap) and a tap in `GhosttyTerminal`'s write paths that copies each decoded write with `copyToClipboard`. Reads are refused. Reference: [terminal-osc52-clipboard.md](../terminal-osc52-clipboard.md).

`GhosttyTerminal.tsx` grew by 12 lines (1467 → 1479); its [code-issue record](../code-issues/oversized-file-ghostty-terminal.md) carries the measurement and the split stays deferred. Backlog entries added: OSC 52 clipboard reachability in Tddy Desktop and insecure origins (unverified); `initialContent` written twice on mount (pre-existing).

Verified (scoped to `tddy-web`): `bun test src/lib` 365 pass; Cypress `GhosttyTerminalOsc52Acceptance` 4, `GhosttyTerminal` 11, `GhosttyTerminalSession` 9, `GhosttyTerminalBufferText` 2, `GrpcSessionTerminalLazyHistory` 1, all passing.

## Validation results

- validate-tests: the bun suite and the Cypress acceptance read as Given-When-Then with a stubbed clipboard, no sleeps or ordering dependence, and each test asserts the copied text or its absence. Fixed: the stale PRD path in two file headers.
- validate-prod-ready: no mock code, TODO/FIXME, debug output or test-only branches in `osc52.ts` or the tap.
- analyze-clean-code: `osc52.ts` is 152 lines, no function over 40 lines apart from the parser closure's `step` switch (the state machine itself), nesting at most 3.
