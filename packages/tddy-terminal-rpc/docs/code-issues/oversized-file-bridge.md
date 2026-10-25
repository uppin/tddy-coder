# oversized-file: bridge.rs

**Location:** `packages/tddy-terminal-rpc/src/bridge.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 on PR #760 (`#term-snapshot` 1/4)
**Metrics:** **682 production lines** (no `#[cfg(test)]` block) · budget 500 · **1.3× over**
**Thresholds breached:** length 670 > 500
**Restructure:** `extract_module --to_file` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Related:** `complexity-bridge-open-replay-ack-live.md`, `complexity-bridge-spawn-live-bridge.md` (all in this file)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 670 | first detection; already 561 on `origin/master`, 561 → 670 in PR #760: the `SNAPSHOT` open (`open_snapshot`), the snapshot and refusal frames on `FrameIdentity`, offset stamping on live frames and the input refusal frame. Decomposition deferred with the developer's consent: the file is stack-shared — `#term-snapshot` 2/4–4/4 (#761, #762, #763) modify it too |
| 2026-10-25 | 682 | +12 later in PR #760: the bidi input forwarder moved into its own `spawn_input_forwarder` (watchers open without the lease; every data chunk is judged on its own). Still deferred, same reason |

## What the tool found

The file holds the frame constructors (`FrameIdentity`), the three open modes (TAIL / FROM_OFFSET in
`open_replay_ack_live`, SNAPSHOT in `open_snapshot`), the live bridge, and the bidi entry point with
its input forwarder.

## Why it matters here

The open modes and the live bridge are where every terminal-wire change lands; a 670-line file with
three functions at nesting 6 is read in full for each.

## What would close it

Split the per-mode opens (`open_replay_ack_live`, `open_snapshot`) from the live bridge and the
bidi input forwarder — e.g. `bridge/open.rs`, `bridge/live.rs`, `bridge/input.rs` — after the
`#term-snapshot` stack lands. Anchor and prove the seams with `tddy-tools restructure`.
