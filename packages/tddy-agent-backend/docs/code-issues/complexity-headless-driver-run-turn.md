# complexity: run_turn

**Location:** `packages/tddy-agent-backend/src/backend/headless_driver.rs` — `HeadlessDriver::run_turn`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` step 4 (clean-code scan, fn line to closing brace) on PR #726 (`#coagents` 7/8)
**Metrics:** **91 lines** (`run_turn`; `read_turn` 83) · nesting 4 · 3 parameters · budget 60 / 4
**Thresholds breached:** length 80 > 60
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-25 | 80 | 4 | first detection — new in PR #726: spawn, the stdout/cancel `select!` loop, the post-EOF wait, kill, reap and the ending. Deferred to the `#coagents` sweep node by the developer (2026-10-25) |
| 2026-11-05 | 91 (`run_turn`) · 83 (`read_turn`) | 4 | **regressed** by #821 (`#agent-tools` 15/19): `run_turn` renders every turn through the spec renderer (`render_env`, the transcript factory, the scratch directory), and the stdout/cancel `select!` loop lives in `read_turn`, which gained the turn-deadline arms at wrap (`end_overrun`). Deferred to the `#agent-tools` sweep node (n20) |

## What would close it

Extract the read loop plus the post-EOF wait (`select!` over `cancel` and `lines` / `child.wait()`)
into `async fn read_until_end_or_cancel(&mut lines, &mut child, &cancel, &mut transcript) ->
Result<bool, BackendError>`, and the stderr/ending assembly into a small helper; `run_turn` then
reads as spawn → read → reap → finish.

## Measurement update 2026-11-02 — #agent-tools 6/12 (#795)

**Moved:** the spawn-free read loop of `run_turn` became `HeadlessDriver::read_turn` (shared with the new `run_rendered_turn`, 53 lines). `read_turn`: **81 lines**, budget 60. **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02).

## Measurement update 2026-11-05 — #agent-tools 15/19 (#821)

**Regressed:** `HeadlessDriver::run_turn` is now the one turn path (the renderer-less `run_turn` and `run_rendered_turn` merged, `argv.rs` deleted): **91 lines**, nesting 3, budget 60 — render, transcript, write, env, spawn. `read_turn`: **73 lines** (was 81). Still deferred to the #agent-tools sweep node; the same extraction closes it: `HeadlessTurn` assembly and the env rendering into a `headless_driver/rendered.rs` sibling.
