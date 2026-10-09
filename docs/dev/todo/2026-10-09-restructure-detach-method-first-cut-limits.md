# 2026-10-09 — `detach_method` first cut: what it refuses instead of handling

**Category:** Future enhancement (refusals a later cut could turn into rewrites)
**Source:** #reshape 18/19 (`feature/reshape/backend-session`), which adds `detach_method` and needs none of these for the
Rust backend.

## Refused today, each before anything is written

1. **A generic `impl`, or one with a `where` clause** (DS2). The function would have to restate the impl's parameters and
   bounds, and every turbofish at a caller would move.
2. **A `pub` member** (DS4). It may have callers outside the workspace, which no reference survey sees. Detaching it is
   a breaking API change that should be asked for by name, for example with a variant that keeps a forwarding method, like
   `retarget_impl`'s `leave_delegator`.
3. **A receiver behind a smart pointer** (`Box<T>`, `Rc<T>`, `Arc<Mutex<T>>`…) (DS8). The call relies on auto-deref, so
   the rewrite would have to write `&mut *r` or `&**r` by the pointer's shape.
4. **A reference inside a macro invocation** (DS9). The tokens are rewritable, but the server's reference inside a macro
   is not always the written token.

## Why deferred

The Rust backend, the only planned user, has none of these: every receiver is `&mut RustBackend`, no member to detach is
`pub`, `impl RustBackend` is not generic, and no call sits in a macro (#reshape 18 discovery § 6). Each refusal names the
member or site, so a later plan that hits one says so.
