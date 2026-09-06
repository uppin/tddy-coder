# 2026-09-06 — `server.rs::run_server` takes 12 positional arguments

**Category:** Deferred from `optional-livekit` (#449)
**Source:** optional-livekit common-room switch, #449

It already carries
`#[allow(clippy::too_many_arguments)]`; #449 added the twelfth (`livekit_enabled`). An options
struct is the right fix, but it moves `main.rs` and the desktop caller, and `tddy-desktop` is
outside the CI gate — so it wants its own PR, after the `optional-livekit` stack lands, where the
desktop build can actually be exercised.
