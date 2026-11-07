# 2026-11-07 — tddy-tauri-web's transport log option is dropped, and the package does not typecheck

**Category:** Bug (silently ignored option) + lint gate gap
**Source:** `#web-split` 6/17 (PR #843), web CI redesign — found by the first `Web lint` run

`createTauriTransport` (`packages/tddy-tauri-web/src/transport.ts:139`) passes `log: options.log` to
`createEnvelopeTransport`, but `EnvelopeTransportOptions` (`packages/tddy-rpc-web/src/envelope-transport.ts`)
has no `log`; only the lower-level `EnvelopeCallOptions` does. `tsc --noEmit` fails with TS2353, and
the caller's log sink never receives a line. Same on `master` — it predates `#web-split`.

**Why deferred:** threading `log` through `createEnvelopeTransport` changes `tddy-rpc-web`'s public
options and the desktop app's logging; it is not part of the CI redesign. The package is on
`scripts/web-lint-exclusions.json` until it is fixed.

**What closing it takes:** add `log?: (message: string) => void` to `EnvelopeTransportOptions` and pass
it to the transport it builds (with a test that a call logs through it), then delete the
`tddy-tauri-web` entry from `scripts/web-lint-exclusions.json`.
