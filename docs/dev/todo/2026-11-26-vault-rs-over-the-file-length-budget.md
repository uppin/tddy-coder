# `vault.rs` is over the file-length budget

**Category:** File length (500 production-line budget)
**Source:** `/pr-wrap` step 3.5 file-length gate on [#1045](https://github.com/uppin/tddy-coder/pull/1045)
**Packages:** `tddy-credentials`
**Files:** `packages/tddy-credentials/src/vault.rs`

## Measurement

`vault.rs` carries 615 production lines (counted to the first `#[cfg(test)]`, per the gate), up
from 581 before #1045. The +34 lines are the filesystem-port plumbing that PRD
`2026-11-26-fs-port-crate` assigns to this file — the `fs: Arc<dyn tddy_fs::Fs>` field on
`SessionVault`, the `create_on` / `open_with_passphrase_on` constructors, and the read/write call
sites switched to the held port — not new behaviour.

## Why it was deferred rather than decomposed in #1045

#1045 is the root of stack `fs-port`; eleven migration nodes wait on it, and none of them touches
this file. Splitting a security-critical vault module mid-streak — through the cold restructure
engine on a tight disk, or by hand against the engine-moves policy — risks destabilising the
crate every later node builds on, to save 34 lines of mechanical, fully tested plumbing. The
decomposition belongs to a follow-up branch after the stack lands, when it can be reviewed on its
own.

## What would close it

Split `vault.rs` (e.g. `CredentialStore` constructors vs `SessionVault` methods) so every
resulting file is under 500 production lines, with the same green suite before and after.
