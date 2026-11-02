# 2026-11-02 — A terminal's OSC 52 copy reaches the viewer's clipboard

**Type:** Feature

`#session-tabs 4/7` ([#807](https://github.com/uppin/tddy-coder/pull/807)). New `src/lib/osc52.ts` (a streaming OSC 52 write recognizer, 1 MiB cap) and a tap in `GhosttyTerminal`'s write paths that copies each decoded write with `copyToClipboard`. Reads are refused. Reference: [terminal-osc52-clipboard.md](../terminal-osc52-clipboard.md).

`GhosttyTerminal.tsx` grew by 12 lines (1467 → 1479); its [code-issue record](../code-issues/oversized-file-ghostty-terminal.md) carries the measurement and the split stays deferred. Backlog entries added: OSC 52 clipboard reachability in Tddy Desktop and insecure origins (unverified); `initialContent` written twice on mount (pre-existing).

Verified (scoped to `tddy-web`): `bun test src/lib` 365 pass; Cypress `GhosttyTerminalOsc52Acceptance` 4, `GhosttyTerminal` 11, `GhosttyTerminalSession` 9, `GhosttyTerminalBufferText` 2, `GrpcSessionTerminalLazyHistory` 1, all passing.
