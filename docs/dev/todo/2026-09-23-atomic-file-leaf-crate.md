# Two copies of "replace a secret without truncating it"

**Category:** Duplication / crate boundary
**Source:** `#keyring` 3/9 validation finding V7 (`docs/dev/1-WIP/2026-09-19-keyring-store.md`)
**Packages:** `tddy-credentials`, `tddy-session-store`
**Files:** `packages/tddy-credentials/src/atomic.rs`, `packages/tddy-session-store/src/atomic_file.rs`

## What is duplicated

`tddy-credentials` writes its vault through `atomic::write_owner_only`: a swap file created
`create_new` at mode `0600`, `fsync`, `rename` over the target, directory `fsync`. That is the same
invariant `tddy_session_store::atomic_file::write_atomic_with_mode` implements (and `tddy-core`
re-exports), reduced to the one mode the vault uses — about 45 lines.

## Why it was copied rather than depended on

`tddy-credentials` used to depend on `tddy-core` for that one function, which pulled tokio,
jsonschema, ACP, the workflow, git, task, RPC and sandbox crates into every consumer of the vault —
including `#keyring` 6/9 (sync) and 7/9 (screen sharing), which the crate is split out for.
`tddy-session-store` is lighter but still brings tokio, jsonschema, `tddy-workflow`, `tddy-actions`
and `tddy-task`. Neither is an honest dependency for a crypto leaf.

## The fix

A leaf crate (e.g. `tddy-atomic-file`, `std` + `rand` or `uuid` only) holding `write_atomic`,
`write_atomic_with_mode` and `write_atomic_labelled`, which `tddy-session-store` re-exports at its
current path and `tddy-credentials` depends on directly. Out of scope for `#keyring` 3/9, which does
not touch `tddy-session-store`.

## Until then

A fix to one copy — a platform quirk, a new failure mode on a full disk — must be made to both.
