# 2026-10-05 — two `#keyring` 6/9 files are over the 500-production-line budget

**Category:** Deferred from `#keyring` 6/9 `credential-sync` (#513)
**Source:** `/pr-wrap` step 3.5's file-length gate
**Consent:** the developer deferred both, 2026-10-05, choosing to decompose the pre-existing
oversized files (`config.rs`, `runtime.rs`) by consent as well rather than restructure mid-PR

| File | Production lines | Note |
|---|---|---|
| `packages/tddy-credential-sync/src/engine.rs` | 529 | new file — `SyncEngine`, the wire-format mirror types (`WireEntry`/`OpenedWireEntry`), their conversions, and the reconciliation/wrap/receive logic, all in one module |
| `packages/tddy-credentials/src/vault.rs` | 581 (was 477 before this PR) | grown by the tombstone-aware `VaultEntry` storage: `EntryToSeal`/`OpenedEntry`, `seal_entry`/`open_entry`, and `remove`/`entries`'s real bodies |

**Why deferred:** both are mid-implementation on a PR already fully green end to end (CI, scoped
tests, clippy, fmt); splitting either now risks destabilizing a feature PR for a decomposition that
is pure code organisation, not behaviour. Neither has a dependent in this stack that would make a
later split a conflict — `#keyring`'s `## Dependencies` lists no node that touches either file.

## What would close it

- **`engine.rs`**: extract the wire-format mirror types and their `From` conversions
  (`WireEntry`/`OpenedWireEntry`, roughly lines 409-511) into their own module (e.g. `wire.rs`),
  re-exported so `SyncEngine`'s own callers are unaffected. That alone should bring the remainder
  comfortably under budget; `admit`/`publish`/`receive`/`reconcile` are one cohesive concern and
  should stay together.
- **`vault.rs`**: extract the private sealing mirrors (`EntryToSeal`/`OpenedEntry` and their `From`
  impls, roughly lines 404-end) into a sibling module, the same seam `RecordToSeal`/`OpenedRecord`
  already suggested before this PR grew them tombstone-aware.

Anchor with `tddy-tools restructure anchors` rather than by hand, and prove the seam with
`restructure check --deep` against a warm index (`./run-index-daemon`). Delete this entry once both
files are back under 500 production lines.
