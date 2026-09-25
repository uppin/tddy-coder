# 2026-09-25 — a cross-crate move's glob facade re-exports the destination's whole root, and a name the origin already has trips `hidden_glob_reexports`

**Category:** Future enhancement (engine defect; the lint gate goes red after the move)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02a-pty-runtime-to-terminal-rpc.jsonl`,
op 0 (`pty_runtime` + `tddy_user_config` → `tddy-terminal-rpc`, `reexport: glob`)

## What happened

`reexport: glob` replaced each moved `pub mod` line in the origin's `lib.rs` with a glob over the
destination's **root**. It did not glob just the moved modules:

```rust
// packages/tddy-session-lifecycle/src/lib.rs, as the engine left it
mod service;                       // lifecycle's own private module, line 4
…
pub use tddy_terminal_rpc::*;      // was `pub mod pty_runtime;`
…
pub use tddy_terminal_rpc::*;      // was `pub mod tddy_user_config;` (the duplicate already recorded)
```

`tddy-terminal-rpc` has a `pub mod service;` of its own, so the glob tries to publish
`tddy_session_lifecycle::service`, and lifecycle's private `mod service;` shadows it:

```text
warning: private item shadows public glob re-export
   --> packages/tddy-session-lifecycle/src/lib.rs:4:1
  4 | mod service;
note: the name `service` in the type namespace is supposed to be publicly re-exported here
   --> packages/tddy-session-lifecycle/src/lib.rs:105:9
105 | pub use tddy_terminal_rpc::*;
```

`cargo check` passes, so `apply`'s compile gate cannot see it, but `clippy -D warnings` fails. Even
without the collision, the root glob publishes every public item of the destination as a new
`tddy_session_lifecycle::…` path, which is more than the move needs to keep resolving.

## What was fixed by hand

The two globs became one line naming the moved modules:

```rust
pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};
```

This is the named facade style lifecycle's `lib.rs` already documents for its other receivers
("Named one by one rather than globbed").

## What would fix it

A crate move's facade could name what moved. For a move of whole modules, that is the module names:
one grouped `pub use <dest>::{a, b};` per plan, rather than one root glob per operation. That also
retires the duplicate-facade cause in
[`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](./2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md).
If the root glob stays, the engine should at least refuse or report a destination root name that an
origin root item already binds.

Moves 1a (`pub use tddy_daemon_kernel::*;`) and 2b (`pub use tddy_daemon_sandbox::*;`) left root
globs that happen to collide with nothing today. A later move into either crate that adds a name
lifecycle already has would trip the same lint.
