# 2026-09-25 — `check` passes a cross-crate move whose module name the destination already defines

**Category:** Future enhancement (engine defect: `check --deep` passes a move that collides)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), move T5b
(`session_notifications` → `tddy-session-activity`). Found by reading the code before any apply, and
confirmed by a diagnostic `check --deep` that reported `no findings`. Nothing was applied.

## What happened

Lifecycle's `session_notifications` is the half of a module that `#unbundle` node 7 split. The other
half already lives in the destination under the **same name**, and the origin globs it back in:

```rust
// packages/tddy-session-activity/src/lib.rs
pub mod session_notifications;          // 403 lines: the bus, the event, the subscriber trait

// packages/tddy-session-lifecycle/src/session_notifications.rs (96 lines)
pub use tddy_session_activity::session_notifications::*;
pub fn resolve_session_label(…) -> String { … }
pub struct SessionNotificationPublishing { … }
```

A `move_module_to_crate` of lifecycle's module into `tddy-session-activity` has nowhere to go:
`src/session_notifications.rs` exists, and the destination's `lib.rs` already declares the name.
The diagnostic `check --deep` listed the survey with every caller re-pointed to
`tddy_session_activity::session_notifications::…` and reported `no findings`:

```text
op 1 survey: packages/tddy-session-lifecycle/src/session_notifications.rs -> tddy-session-activity (tddy_session_activity), 2 item(s) reached from outside, 3 caller(s)
      reached from outside: SessionNotificationPublishing, resolve_session_label
…
no findings
```

Whether `apply` would overwrite the destination's file or refuse was not tested.

## What would fix it

A static finding: the destination's root already binds the moved module's name (a `mod` declaration,
or a file at the target path). It needs no index. A move into a crate that already holds the other
half of a split module is a **merge**, which no operation performs, so the finding should say that
rather than suggest a cluster.
