# 2026-10-22 — an extension language's server is never told of files changed on disk

**Category:** Behaviour gap
**Found in:** `#reshape` 22/24 (`extension-language-server`, PR #681), at `/green`
**Owner:** unassigned — `tddy-index-daemon`

## What happens

`WorkspaceIndex::client_for` tells the warm rust-analyzer of every `*.rs`, `Cargo.toml` and `Cargo.lock` created,
changed or deleted since its last request (`workspace/didChangeWatchedFiles`, `tree_changes.rs`).
`client_for_route` hands an extension language's server out with **no** tree changes: only Rust sources are tracked.
A document the server is asked about is announced afresh with its current text on every request, so the file in
question is always current; but other files of the language that changed on disk — the targets of a references
query, the files a rename's `WorkspaceEdit` reaches — are whatever the server read when it started.

## Why it was not fixed in #681

`TreeSnapshot` walks Rust's file set only, and the set an extension language needs is its `path_suffixes`. No fake
server mode of #681 answers from files it was not told about.

## Wanted

`TreeSnapshot` (or a second snapshot per extension language) over the files the language's support claims, and
`client_for_route` notifying the extension server of the difference as `client_for` does for Rust. Then a
`tree_changes_acceptance.rs` case over an extension language.
