# 2026-09-06 — `server.rs::run_server` takes 12 positional arguments — resolved 2026-09-09

**Category:** Deferred from `optional-livekit` (#449)
**Status:** Resolved
**Source:** optional-livekit common-room switch, #449

**Resolved 2026-09-09** by [#470](https://github.com/uppin/tddy-coder/pull/470), which got the dedicated PR this entry asked for: `run_server` now
takes one `RunServerOptions` and the `#[allow(clippy::too_many_arguments)]` is gone.

Two corrections to what this entry assumed. The struct has **no `Default`** — a defaulted `host: ""`
would fail to bind at runtime instead of failing to compile, so every caller states all twelve
fields. And **`tddy-desktop` is not a caller**: its only mention of `run_server` is prose inside a
`TODO` comment, because the desktop host builds its own `MultiRpcService` and serves over Tauri IPC
with no HTTP listener. Four call sites migrated — `main.rs` and three daemon tests. The desktop was
still built and linted locally, since it is outside the CI gate.


It already carries
`#[allow(clippy::too_many_arguments)]`; #449 added the twelfth (`livekit_enabled`). An options
struct is the right fix, but it moves `main.rs` and the desktop caller, and `tddy-desktop` is
outside the CI gate — so it wants its own PR, after the `optional-livekit` stack lands, where the
desktop build can actually be exercised.
