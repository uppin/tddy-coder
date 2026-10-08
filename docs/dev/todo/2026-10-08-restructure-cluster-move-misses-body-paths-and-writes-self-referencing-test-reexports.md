# 2026-10-08 — a cluster move leaves origin-rooted body paths and self-referencing re-exports in the moved files

**Category:** Engine failure plus manual fixes (build corrections)
**Source:** #carve 21/21 (PR #536), R8: the 16-module split cluster → `tddy-session-split`. Engine causes already filed:
[2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path](2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md),
[2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling](2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)

## What the engine did, and the hand fixes

1. **Self-referencing re-exports** (as in R3/R4): `pub use tddy_session_split::{agent_argv, agent_credentials};` (`split_session.rs:148`),
   `pub use tddy_session_split::split_claude_cli_start;` (`split_start.rs:1`), `pub use tddy_session_split::svc_paired_codebase_teardown;`
   (`svc_spawn_split_agent.rs:434`): the parents' old `mod x;` lines rewritten to facades onto their own crate (`E0432`). **Hand fix: `tddy_session_split::` → `crate::`**
   in the three lines (the children were flattened to root siblings, so `crate::` keeps the old `parent::child` paths resolving).
2. **A body path left rooted at the origin**: `..crate::connection_service::starting_session_metadata(…)` inside a struct-update expression
   (`svc_spawn_split_agent.rs:423`, `workspace_session.rs:140,244`) was not re-pointed (`E0433: could not find connection_service in the crate root`).
   **Hand fix:** `crate::service_util::starting_session_metadata` (the moved module's new path). A doc link in `agent_argv.rs:20` to
   `crate::connection_service::roster_replacement_pairs` was left as written (rustdoc only).
3. The visibility widening is itemised in [2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses](2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses.md).

## What the engine should do

Re-point a body path rooted at a moved module's old parent even inside a struct-update `..path(...)` expression; and never write a facade whose target is the
crate the facade lives in. Delete this file with those fixes.
