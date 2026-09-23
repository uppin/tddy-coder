# 2026-09-19 — The file-length gate measures 43 lines for a 1,949-line file

**Category:** Defect
**Source:** `/pr-wrap` step 3.5 on PR #518, cross-checked against `/analyze-clean-code`

`/pr-wrap` step 3.5 and every `packages/*/docs/code-issues/oversized-file-*.md` record measure Rust
files as **production lines counted to the first `#[cfg(test)]`**:

```bash
awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++} END {print n+0}'
```

That heuristic assumes the first `#[cfg(test)]` opens the test module at the bottom. It does not
have to. `packages/tddy-session-lifecycle/src/connection_service.rs` puts **`#[cfg(test)] use`
declarations at line 44**, for imports only its extracted test modules need:

```rust
// Bound for the extracted test modules, which reach the code under test through `use super::*`.
// … `#[cfg(test)]` keeps them out of the lib build entirely rather than trading a resolution
// error for a lint.
#[cfg(test)]
use crate::livekit_peer_discovery::local_instance_id_for_config;
```

So `awk` exits at line 44 and the gate reports **43 production lines** for a file whose real
production content runs to about **line 1,930** — `DaemonSessionHost` at `:130`, `classify_placement`
at `~:1045`, and 20+ `mod` declarations in between. The actual test module starts at `:1931`.

## Why it matters

- **The file is invisible to the gate.** It has never been flagged, has no
  `oversized-file-connection-service.md` record, and PR #518 added 87 lines to it without the gate
  saying a word.
- **It is silent, not loud.** An unresolvable base aborts the gate with `GATE ERROR`; this
  under-measures and passes.
- **Any file can opt out accidentally** by putting a test-only `use` near the top — which is itself
  good practice, and which `connection_service.rs`'s own comment recommends.

## What would close it

Count production lines as *total lines minus lines inside `#[cfg(test)]` items*, rather than
*lines before the first one*. A cheap approximation that fixes this case: skip a `#[cfg(test)]` whose
next non-attribute line starts with `use`, and only stop at one that opens a `mod`. The robust
version is a syntax-aware count — `tddy-tools analyze` already parses these files.

Until then, a file with early `#[cfg(test)] use` lines needs its record's number taken by hand, and
the record should say so.

## Answered in part — 2026-09-23 (#520, `#carve` 11/12)

The cheap approximation above — stop only at a `#[cfg(test)]` whose next line opens a `mod` — is
what `packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs` uses to measure crate production lines
(AC11/AC12 of #520), with `*_tests.rs`, `tests.rs` and `test_util.rs` excluded. **The `/pr-wrap`
step 3.5 gate itself is unchanged** and still exits at the first `#[cfg(test)]` of any kind.

The approximation is **not enough for this file**, which settles that it is not a fix for the gate:
`connection_service.rs` interleaves out-of-line `#[cfg(test)] mod x_tests;` declarations with its
production items, so the mod-aware cut stops at `#[cfg(test)] mod stack_child_spawn_tests;` at
`:817` and reports **816** production lines, where excluding each `#[cfg(test)]` item individually
gives **1,569** (1,647 total; production items continue to `:1618`). What would close this entry is
therefore the robust version only: exclude `#[cfg(test)]` items one by one (syntax-aware, or at least
attribute-plus-item), rather than cutting the file at any single line. The crate-level measurements
of #520 used the mod-aware cut on both sides of the change, so its deltas compare like with like even
where the absolute numbers undercount.

## Verified by hand

2026-09-19: `count_prod` returns 43; `grep -n` shows production items at `:62`, `:67`, `:130`, `:233`,
`:242`; the last `#[cfg(test)]` is at `:1948` and `wc -l` is 1,949.
