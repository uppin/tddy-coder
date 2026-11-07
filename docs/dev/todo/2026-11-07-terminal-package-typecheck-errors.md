# 2026-11-07 — `tddy-terminal-web` does not typecheck (17 errors)

**Category:** Type errors (moved tests, sibling packages)
**Source:** `#web-split` 8/17 (PR #845), `/validate-changes`
**Detected:** `cd packages/tddy-terminal-web && ../../dev bunx tsc --noEmit -p .` — 17 `error TS` lines
at `feature/web-split/terminal`, 2026-11-07: 8 in `cypress/` (RefObject nullability in
`GhosttyTerminalBufferText.cy.tsx`, upload-response literals without `$typeName`, Sinon `.as` in
`ghosttyTerminalSessionDriver.tsx`), 5 in `src/**/*.test.ts` (`remoteTerminateConfirm.test.ts` window
cast, `requestSnapshot` in `terminalFeed.test.ts`), 1 in `tddy-client-web/src/lib/debugMask.ts` (no
declaration for `debug`), 1 in `tddy-tauri-web/src/transport.ts` (`log` not in
`EnvelopeTransportOptions`). Production `src/` is clean.

**Why deferred:** `tsc` is not a gate in this repo, the specs pass, and all of it is test code moved
verbatim from `tddy-web` or code in sibling packages — not this node's move.

**What closing it takes:** typed fixtures (`create(<Schema>, …)`), nullable-ref helpers in the
driver, `@types/debug` (or a local declaration), the `log` option on `EnvelopeTransportOptions` — then
`bun run typecheck` in the package's PR check.
