# 2026-10-09 — `tddy-session-lifecycle/src/lib.rs` documents `tddy_terminal_rpc` with the moved presenter observer's doc comment

**Category:** Hygiene (a stranded doc comment left by an engine move, on master)
**Source:** #reshape 3/19 (`feature/reshape/tidy-facades`), found while tracing the orphan-doc defect of #carve 21/21 (PR #536)

`packages/tddy-session-lifecycle/src/lib.rs:101-103`: the two `///` lines describing "the per-session presenter
observer" sat above `pub mod presenter_observer_task;`. The R4 cluster move removed that declaration line alone, so
the doc now documents `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};`, which it does not describe.

**What to do:** move the two lines to `packages/tddy-session-activity/src/lib.rs` above `pub mod
presenter_observer_task;` (or delete them, since `presenter_observer_task.rs` carries its own `//!` docs). One hand
edit, docs only.

**Why deferred:** the engine fix (#reshape 3) stops new strandings but does not repair old ones, and this file belongs
to another package; it is not a restructure.
