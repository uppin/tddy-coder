# 2026-10-05 — `restructure` cannot re-point an import that goes through a facade to the crate that defines it, so 12 files were edited by hand

**Category:** Future enhancement (missing capability; nothing breaks, the edits are done by hand)
**Source:** #carve 17/21 (PR #532) stage B3, acceptance check A4 (M4.6)

## What I did by hand

`tddy-session-lifecycle`'s `lib.rs` re-exports modules of other crates (`pub use tddy_daemon_kernel::config;`,
`pub use tddy_session_files::{session_file_upload, host_documents, …};`, …), so a file in the crate can name
`crate::config::DaemonConfig` and the item resolves to `tddy_daemon_kernel`. A module that is to move into
another crate must name what it uses by the crate that defines it, or the move presents the shape
[`2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge`](2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md)
refuses. Re-pointing is mechanical: for every path whose first segment after `crate::` is a facade, replace
`crate::<facade>` by the facade's target path.

12 files, 30 source lines, by hand (rustfmt reflowed some): `crate::config` and `crate::user_sessions_path` ->
`tddy_daemon_kernel::{config, user_paths}`; `crate::livekit_peer_discovery` -> `tddy_daemon_livekit::…`;
`crate::session_agent_clone` / `session_agent_status` -> `tddy_session_agents::…`; `crate::session_file_upload`,
`session_attachment_staging`, `host_documents` -> `tddy_session_files::…`; `crate::session_list_enrichment` and
the items `crate::session_notifications` glob-re-exports -> `tddy_session_activity::…`. Four of the paths sat
inside a grouped `use crate::{a::b, c}`, which a text search for `crate::<facade>` does not find.

## What would have made it unnecessary

An operation (`repoint_facade_imports`, anchored on a file or a module) that, for each path in the anchor whose
head resolves through a `pub use` of another crate, rewrites the head to the defining path rust-analyzer
reports (`goto_definition` on the head segment, then the shortest path from the other crate), including heads
inside a grouped `use`, leaving every comment and every own-module path alone. A `check --deep` that lists the
paths it would rewrite would also give the acceptance check (A4) a mechanical answer in place of the grep,
which is blind to grouped imports and to a facade the grep's pattern did not list.

## Minimal reproduction

```rust
// lib.rs
pub use other_crate::config;
// a.rs
use crate::config::Settings;          // -> use other_crate::config::Settings;
use crate::{config::Limits, b::Thing}; // -> use other_crate::config::Limits; use crate::b::Thing;
```
