# 2026-11-18 — The Coder one-shot transport lives in its own module

**Type:** Refactor · `#coder-start` 4/4, PR [#944](https://github.com/uppin/tddy-coder/pull/944)

The one-shot wrapper (start marker, counted base64 stdin lines, the wrapper command line, the
marker-noting reader) moved from `src/coder.rs` to `src/coder/one_shot.rs`, applied through
`tddy-tools restructure`. `coder.rs` went from 507 to 405 production lines (`one_shot.rs` is 107),
under the 500-line budget. No behaviour or public-surface change: five of the moved items are
`pub(super)`, read by `CoderShell` in the parent module. Closes the `oversized-file-coder` code issue.

Full record: [`docs/dev/changesets/2026-11-18-coder-start-sweep.md`](../../../../docs/dev/changesets/2026-11-18-coder-start-sweep.md).
