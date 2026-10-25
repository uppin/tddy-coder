# complexity: run_turn

**Location:** `packages/tddy-agent-backend/src/backend/headless_driver.rs` — `HeadlessDriver::run_turn`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` step 4 (clean-code scan, fn line to closing brace) on PR #726 (`#coagents` 7/8)
**Metrics:** **80 lines** · nesting 4 · 3 parameters · budget 60 / 4
**Thresholds breached:** length 80 > 60
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-25 | 80 | 4 | first detection — new in PR #726: spawn, the stdout/cancel `select!` loop, the post-EOF wait, kill, reap and the ending. Deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What would close it

Extract the read loop plus the post-EOF wait (`select!` over `cancel` and `lines` / `child.wait()`)
into `async fn read_until_end_or_cancel(&mut lines, &mut child, &cancel, &mut transcript) ->
Result<bool, BackendError>`, and the stderr/ending assembly into a small helper; `run_turn` then
reads as spawn → read → reap → finish.
