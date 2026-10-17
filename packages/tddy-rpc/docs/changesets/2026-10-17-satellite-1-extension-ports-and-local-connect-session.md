# 2026-10-17 — tokio's rt feature is declared

**Type:** Fix

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

The crate's `tokio` dependency enables `rt`, which it needs for `tokio::spawn`. It compiled only through
feature unification and failed when built alone, which the first crate with no other tokio user
(`tddy-daemon-extensions`) exposed.
