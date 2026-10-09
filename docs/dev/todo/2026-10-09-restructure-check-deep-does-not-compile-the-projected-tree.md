# 2026-10-09 — `check --deep` does not compile the projected tree

**Category:** Future enhancement — check/apply parity
**Source:** `#reshape` 2/19 (`feature/reshape/multi-seam-extract`), the third "possible fix" of the
deleted `2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`

`#reshape` 2 made every operation of a `check --deep` resolve against the plan's projected tree (the
files earlier operations wrote are shown to rust-analyzer), so a deep check now reaches the same
refusals and widenings as `apply`. It still does not **compile** that tree. A clean deep check can be
followed by an `apply` whose compile gate fails for reasons the engine's passes do not see — the
2026-09-24 apply-gaps record lists them (I, K, M, W, G).

## Possible fix

After the last operation is rehearsed, materialise the `Overlay` into a scratch copy of the affected
packages (or a `git worktree` at `HEAD` with the overlay written over it) and run the same
`cargo check` the compile gate runs (`runner/compile_gate.rs`), reporting its errors as findings.

## Why deferred

Cost and shape. A scratch copy of a 1,000-crate workspace, or a worktree plus a cold `target/`, turns
a seconds-long deep check into minutes, and sharing `target/` with the real tree races other runs.
`#reshape` 2's mechanism closes the multi-seam defect without it; this is the remaining gap between
"resolves the same" and "compiles the same". Needs the developer's call on cost versus guarantee.
