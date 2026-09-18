# oversized-file: telegram_session_control.rs — one 2,634-line impl

**Location:** `packages/tddy-session-lifecycle/src/telegram_session_control.rs`
**Category:** oversized-file
**Detected:** 2026-09-15 by structural audit
**Metrics:** **3,980 production lines** (4,476 total) · one `impl` block spanning **1272–3906** · **57 methods** · budget 500
**Restructure:** required — `extract_module --to_file` × 7
**Status:** Open — claimed by #494, in flight
**Claimed by:** #494 — `#carve` 8/10 `telegram` · draft · `feature/carve/telegram`
**Lands after:** #488, #489, #490, #498, #491, #492, #493

## Measurement history

| Run | Production lines | Impl span | Methods | Note |
|---|---|---|---|---|
| 2026-09-15 | 3,980 | 1272–3906 | 57 | first detection |

## What the tool found

~1,200 lines of DTOs and ~30 pure `parse_*` functions, then a **single
`impl TelegramSessionControlHarness` block** from 1272 to 3906 holding 57 methods in six visible
clusters: keyboard pickers (intent → project → branch → conflict → model → agent), elicitation,
session start, chaining, recipe/plan review, and list/delete/enter.

## Why it matters here

The largest single file in the crate, and the one with the most operator-facing behaviour. Every
Telegram command path is in one impl, so any change to one flow is reviewed against 57 methods.

## What would close it

Seven modules: `callbacks` (~700, the pure `parse_*` functions — the cheapest seam, and lifting it
first shrinks the file before anything harder is attempted), `commands` (~150), `spawn` (~220),
`pickers` (~600), `elicitation` (~310), `session_start` (~700), `session_admin` (~400). Target: no
module over 800 production lines.

## If you are about to change this code

#494 splits this file **and** moves it to a new `tddy-telegram-control` crate. That is two
disruptions, and it makes this one of the more expensive files in the repo to change concurrently.

Adding a command or callback handler here means #494 must place it in one of seven modules and carry
it across a crate boundary. If your change is small, say so and it is probably fine; if it adds a
flow, waiting or coordinating is worth considering.

## Verified by hand

2026-09-15: listed all 57 methods with line numbers and confirmed the single `impl` span. Confirmed
the DTO/parser prefix is genuinely free of `self` — those ~30 functions are pure and move cleanly.
