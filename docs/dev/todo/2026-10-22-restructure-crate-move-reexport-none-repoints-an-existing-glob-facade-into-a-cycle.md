# 2026-10-22 — `move_module_to_crate` with `reexport: none` re-points an existing glob facade, closing a cycle

**Category:** Future enhancement (engine gap)
**Source:** `2026-10-22-telegram-satellite` (Telegram as a satellite), green phase — the move of
`pending_elicitation_for_session_dir` from `tddy-telegram` to `tddy-session-activity`

## What happened

`pending_elicitation_for_session_dir` had to leave `tddy-telegram` for `tddy-session-activity` (which
must stop depending on `tddy-telegram`). Two plans:

1. `extract_module` (`name: "pending_elicitation"`, `to_file: true`, `reexport: "glob"`) over the one
   item in `packages/tddy-telegram/src/elicitation.rs`. Its callers are in another crate, so the facade
   was needed for the extraction itself; it left `mod pending_elicitation; pub use pending_elicitation::*;`.
2. `move_module_to_crate` of `elicitation::pending_elicitation` to `packages/tddy-session-activity`,
   **`reexport: "none"`**. `check --deep` reported `no findings` (survey: `1 item(s) reached from
   outside, 0 caller(s)`, and `reached through a glob re-export, not by path: … pub use pending_elicitation::*;`).

The apply moved the file byte-identically, then failed its compile gate:

```text
error: cyclic package dependency: package `tddy-session-activity v0.1.0 (…)` depends on itself.
```

because despite `reexport: "none"` it re-pointed the glob facade from plan 1 instead of removing it, and
added the edge that facade needs:

```rust
// packages/tddy-telegram/src/elicitation.rs, after the apply
pub use tddy_session_activity::pending_elicitation::*;
```

```toml
# packages/tddy-telegram/Cargo.toml, after the apply
tddy-session-activity = { path = "../tddy-session-activity" }
```

`tddy-session-activity` already depends on `tddy-telegram`, so the destination now depended on the crate
it left. The two callers in `tddy-session-activity/src/session_list_enrichment.rs`
(`tddy_telegram::elicitation::pending_elicitation_for_session_dir`) were not re-pointed either: the survey
counted no callers, because they sit in another crate and reach the item through the origin's glob.

## Workaround used

Post-move hand fixes: the re-pointed `pub use` line and the added `tddy-session-activity` manifest line
were removed, and the two callers were re-qualified as
`crate::pending_elicitation::pending_elicitation_for_session_dir`.

## Fix

- With `reexport: "none"`, a glob or named `pub use` of the moved module in the origin is a facade the
  plan asked not to keep: remove it rather than re-point it, and never add a manifest edge from the origin
  to the destination for it.
- When the destination already depends on the origin, any origin → destination edge is a cycle; refuse
  it in `check --deep` (which today reports `no findings`), not only at the compile gate.
- Count callers in **other** workspace crates that reach the item through the origin's re-export, and
  re-point them — here they are in the destination crate itself, so the right path is `crate::…`.
