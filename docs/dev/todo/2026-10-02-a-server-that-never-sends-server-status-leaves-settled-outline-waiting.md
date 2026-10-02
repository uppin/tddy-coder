# 2026-10-02 — A server that never sends `experimental/serverStatus` leaves `settled_outline` waiting

**Category:** Deferred from `item-anchors` (#537)
**Source:** `/pr-wrap` validation of #537 (`#live-plan` 1/7)

`settled_outline` (`packages/tddy-code-restructuring/src/backends/rust.rs:1488`) believes an empty
`documentSymbol` answer when the outline has items, the index is marked loaded, or the server has been
*observed* quiescent. A language server that never sends `experimental/serverStatus` is never observed
quiescent, so a file whose outline is empty (comments only) waits until the caller's cancellation
token fires, then fails with `IndexingIncomplete`. The wait has no deadline of its own.

rust-analyzer sends the status, so this affects only a different server or an older one. A bounded wait
would be a budget, which the restructuring library deliberately withdrew in favour of cancellation
("a run waits until it succeeds or its caller stops it").

**Why deferred.** The two answers conflict with that decision: a deadline here is a new budget, and
treating silence as quiescence is a fallback the repo forbids without developer consent. It needs the
developer to choose. Not reproducible against rust-analyzer.
