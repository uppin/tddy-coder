# oversized-file: lib.rs

**Location:** `packages/tddy-tool-engine/src/lib.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **787 production lines** (of 846 total; the `#[cfg(test)]` module opens at `:788`) ·
budget 500 · **1.6× over**
**Thresholds breached:** length 787 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 793 | first detection |
| 2026-09-26 | 787 | after the M2/M5 fix: the three spawn sites became `contained_shell.rs` and the read window `read_window.rs`, so the two additions left the parent six lines *shorter* rather than longer |
| 2026-09-26 | 789 | **improved.** Two extractions — `contained_shell.rs` (128) and `read_window.rs` (46) — took out more than the shell hardening and `Read` windowing put in |

## What the tool found

Hand measurement: 793 production lines. The `#[cfg(test)]` at `:741` is an inner attribute inside a
function; the test module is at `:794`.

**Not measured:** complexity, nesting, CRAP. `packages/tddy-tool-engine/docs/code-issues/` did not
exist before this record — the package had never been analyzed.

## Why it matters here

This is the crate every workspace tool call lands in, and the file holds the whole dispatch: the
`execute_tool` match `:217-244`, the ten tool implementations, the LSP bridge `:250+`, the shell
paths (`ShellTaskBody` `:120-152`, the blocking path `:505-551`), `tool_await`, and the task
registration helpers.

Two defects it hid, both found by reading rather than by any tool, **both now fixed**:

- ~~**Four unhardened spawn sites** (`:128-135`, `:523-528`, `shell.rs:88-95`, and `tool_grep`)
  each construct `tokio::process::Command` with neither `.stdin(Stdio::null())` nor
  `.kill_on_drop(true)`.~~ Closed 2026-09-26: all four now go through `contained_shell`, the
  single spawn that nulls stdin, spawns into its own process group and signals that group on
  overrun.

  **The first sweep found three and declared itself complete.** On 2026-09-26, `/green` fixed
  `:128-135`, `:523-528` and `shell.rs:88-95`, and this record was marked closed. Later the same
  day a PR #545 validator found the **fourth**: `tool_grep` was still spawning `rg` directly, with
  stdin inherited, no `kill_on_drop`, no process group **and no budget of any kind** — worse than
  the three, which at least had a `tokio::time::timeout` around the wait. `Grep` is an advertised
  tool dispatched through this engine, so it sits on the same in-jail path as the incident. The
  sweep missed it because it searched for the *shell* spawns named in the incident rather than for
  every `tokio::process::Command` in the crate. Fixed on 2026-09-26 via
  `contained_shell::run_contained_argv` (an argv entry point, because shell-quoting a user-supplied
  regex would be a defect of its own) with a 30s budget matching the blocking `Shell` default, and
  covered by a fourth test in `tests/shell_containment_red.rs`.
- ~~**`tool_read` `:302-320` ignores `offset` and `limit`**, which `catalog.rs:21` has advertised
  since the catalog was written.~~ Closed 2026-09-26: `tool_read` applies
  `read_window::line_window` and answers `{content, truncated, total_lines}`.

The length itself remains open.

## What would close it

The shell surface is the natural seam — `ShellTaskBody`, the blocking shell path, `tool_await` and
the task-registration helpers form one concern and would take roughly 250 lines out, leaving the
parent near 540. (The 2026-09-26 fix already lifted the *spawn* out of that surface into
`contained_shell.rs`; the dispatch around it is still here.) A second, smaller extraction of the
LSP bridge (`tool_lsp` and friends) clears it.
Anchor with `tddy-tools restructure anchors`; prove with `restructure check --deep` against a warm
index.

**Deliberately not restructured by the changeset that detected it** —
[`2026-09-26-subagent-turn-control-and-honest-tool-failure`](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
fixes both defects named above and records the length rather than splitting, to keep one
reviewable PR. It consolidated the three spawn sites into `contained_shell.rs` and put the read
window in `read_window.rs`, which removed the duplication and — unexpectedly — took the parent
from 793 to 787 rather than growing it.

## Verified by hand

2026-09-26 — Read the file and `shell.rs`. Confirmed the `:741` `#[cfg(test)]` is an in-function
branch, not the module, so 793 is the real production figure. Confirmed all three spawn sites lack
both `stdin` and `kill_on_drop`, and cross-checked tokio 1.53.1's
`Command::output()` (`src/process/mod.rs:1065-1072`) to establish that stdin is inherited rather
than nulled. Confirmed `tool_read` never reads `offset` or `limit`. Did **not** run
`restructure check`, so the split estimates are arithmetic.

2026-09-26 (`/green`, M2 + M5) — Re-measured after the fix: 787 production lines of 846 total,
`#[cfg(test)]` at `:788`. Both named defects verified closed by
`packages/tddy-tool-engine/tests/shell_containment_red.rs` (3 tests) and
`packages/tddy-tool-engine/tests/read_window_engine_red.rs` (4 tests), all green. The length breach
is untouched and this record stays **Open**.

2026-09-26 (PR #545 review) — The spawn-site count above was **wrong when it was written**: three
were found and fixed, and a fourth, `tool_grep`, was in the file the whole time. It has now been
routed through `contained_shell::run_contained_argv` and `shell_containment_red.rs` carries a
fourth test, verified red against the unfixed call (the test hangs, because the raw spawn had no
budget to time out on) and green after. Re-grepped the crate for `process::Command` afterwards:
`contained_shell.rs` is the only remaining constructor in production code.
