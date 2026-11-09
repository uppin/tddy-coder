# 2026-11-07 — tddy-terminal-web's moved code does not typecheck, so `Web lint` excludes it

**Category:** Lint gate gap
**Source:** `#web-split` 8/17 (PR #845), first `Web lint` run on the package

`tsc --noEmit` in `packages/tddy-terminal-web` reports **17** errors, all in code moved from `tddy-web`
(which never type-checked) or in packages it compiles through:

- `cypress/component/GhosttyTerminalBufferText.cy.tsx` (5): `RefObject<GhosttyTerminalHandle | null>`
  passed where a non-null ref is expected.
- `MobileTerminalUploadButton`, `TerminalDropInsertHostPath`, `TerminalFileDropUpload`,
  `TerminalNativeFileDrop` specs (4): `{ hostPath }` plain objects where `UploadSessionFileChunkResponse`
  is expected.
- ~~`cypress/support/drivers/ghosttyTerminalSessionDriver.tsx` (1): `.as` on a `SinonStub`~~ — fixed in
  #860 (`.as` before `.returns`).
- `src/lib/remoteTerminateConfirm.test.ts` (3): a hand-built `window` and a `delete` of a required member.
- `src/rpc/terminalFeed.test.ts` (2): `requestSnapshot` read from a feed options type that lacks it.
- ~~`../tddy-client-web/src/lib/debugMask.ts` (1, TS7016): no types for `debug`~~ — fixed in #847
  (`@types/debug` in `tddy-client-web`); 16 remained, 15 after #860.
- ~~`../tddy-tauri-web/src/transport.ts` (1, TS2353)~~ — fixed in #852 (`EnvelopeTransportOptions.log`); 14 remain.

**Why deferred:** test-support typing in moved code, not this node's move; the last two belong to
other packages' records.

**What closing it takes:** fix the 14 local errors, then delete the
`tddy-terminal-web` entry from `scripts/web-lint-exclusions.json`.
