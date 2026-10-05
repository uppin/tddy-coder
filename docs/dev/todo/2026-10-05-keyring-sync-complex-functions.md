# 2026-10-05 — two `#keyring` 6/9 functions are over `/analyze-clean-code`'s thresholds

**Category:** Deferred from `#keyring` 6/9 `credential-sync` (#513)
**Source:** `/pr-wrap` step 4 (`/analyze-clean-code`)
**Consent:** the developer deferred both, 2026-10-05, rather than refactor mid-PR

| Function | Metric | Current | Threshold |
|---|---|---|---|
| `tddy_credential_sync::engine::SyncEngine::publish` | length | 73 lines | 60 |
| same | nesting depth | 5 | 4 |
| `tddy_daemon::credential_sync::build` | parameter count | 6 | 5 |

**Why deferred:** `publish` is the function carrying the resend-dedup fix found during this same
PR's review (keying on `written_at()` instead of `version()` — see this changeset's Validation
Results), and it is exercised by the full `credential_propagation_acceptance` suite plus the
dedicated regression test for that fix. Refactoring it now, on a PR already fully green end to end,
risks reintroducing exactly the bug just found there. `build`'s six parameters are the real pieces
`runtime::build` already has in scope at its call site; grouping them costs a type nobody else
needs yet.

## What would close it

- **`publish`**: extract the per-peer delivery (admit, wrap, send, journal the outcome) into its
  own method taking one peer's advertisement and the vault entries, returning what `publish` folds
  into the journal — the same shape `wrap_for` already is for the sealing half. Verify with the
  full `tddy-credential-sync` test suite before and after; nothing in its behaviour should change.
- **`build`**: group `registry`, `room_slot` and `common_room` — the three pieces that come from
  `runtime::build`'s own LiveKit-room assembly — into a small `CommonRoomHandles` (or similar)
  struct, leaving `config`, `signing_key` and `local_instance_id` as their own parameters. Check
  whether `LiveKitPeerTransport::new`'s own three-of-the-same parameters are worth the same grouping
  at the same time, since it takes the identical trio.

Anchor and verify with `restructure check --deep` against a warm index (`./run-index-daemon`) if
using `code-restructuring`; otherwise a careful hand-edit is small enough here that the engine may
not be needed. Delete this entry once both are back under their thresholds.
