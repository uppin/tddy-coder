# 2026-10-02 — `locate_symbol` waits on an empty outline with no deadline

**Category:** Deferred from `item-anchors` (#537)
**Source:** `/pr-wrap` validation of #537 (`#live-plan` 1/7)

`RustBackend::locate_symbol` (`packages/tddy-code-restructuring/src/backends/rust.rs:1908`) loops on
`documentSymbol` while `self.indexed || !outline_is_empty(&symbols)` is false: an empty outline is
believed only once `indexed` is set, and `indexed` is set by `ensure_indexed` (`backends/rust/readiness.rs`).
#537 fixed the same shape for `settled_outline` (`rust.rs:1488`), which now also believes an empty
outline once the server was observed quiescent. `locate_symbol` has no such exit, so a file whose
outline is genuinely empty, reached with `indexed` unset, waits until the caller's cancellation token
fires. Whether a production path reaches it with `indexed` unset was not established.

It is not on the `anchors` path, so #537's acceptance tests do not reach it; it serves symbol
lookups for `symbol` anchors.

**Why deferred.** Changing the predicate changes when a symbol operation is refused, on code this
change did not otherwise touch, and nothing reproduces a hang through it yet. The fix is to route
`locate_symbol` through `settled_outline`, then run the symbol-operation acceptance suites.

**Not blocking** anything in the `#live-plan` stack.
