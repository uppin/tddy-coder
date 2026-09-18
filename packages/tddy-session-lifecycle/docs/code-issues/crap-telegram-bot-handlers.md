# crap: telegram_bot.rs — the crate's two worst functions, both untested

**Location:** `packages/tddy-session-lifecycle/src/telegram_bot.rs:368` `telegram_callback_handler`, `:211` `telegram_message_handler`
**Category:** CRAP
**Detected:** 2026-09-09 by `analyze coverage` + `report` (recorded in `docs/dev/todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md`); re-confirmed 2026-09-15 by structural audit
**Metrics:** CRAP **7,832** (complexity 88) and **2,756** (complexity 52) · both **never executed by any test** · 130/329 functions in the file never run
**Restructure:** no — needs **tests first**, then decomposition. Not a `/code-restructuring` job
**Status:** Open — **unclaimed**

## Measurement history

| Run | CRAP | Complexity | Coverage | Note |
|---|---|---|---|---|
| 2026-09-09 | 7,832 / 2,756 | 88 / 52 | 0% | first CRAP report the repo produced |
| 2026-09-15 | unchanged | 88 / 52 | 0% | re-confirmed; **#494 relocates them unchanged** |

## What the tool found

The first CRAP report captured 2,159 tests and 9,064 instrumented functions, of which 2,531 (27.9%)
are never executed. These two are the worst in the crate by a wide margin. Because CRAP treats
coverage as a boolean, both scores are exactly `cx² + cx` — the ranking is complexity among functions
no test enters.

## Why it matters here

`telegram_callback_handler` is the entry point for **every** inline-keyboard interaction — plan
review, elicitation answers, recipe and model selection, branch conflict resolution. A complexity-88
function with no test is where a mis-routed callback silently answers the wrong prompt.

## What would close it

**Tests before decomposition, and in that order.** Splitting an untested complexity-88 dispatcher
moves the risk without reducing it; the split is only safe once something catches a mis-route.

`missing-tests` is never a delete: these are the live production entry points.

## Why this one is NOT claimed, deliberately

**`#carve` 8/10 (#494) moves these functions to `tddy-telegram-control` unchanged and still
untested.** That is a deliberate boundary, recorded in its changeset: decomposing or testing them
inside a move would put behaviour risk into a node whose entire claim is that it has none, and would
make the diff unreviewable.

So this issue **outlives the refactor**. After #494 it will need `**Moved:**` set to the new crate
and this section replaced — but the finding itself stands, unowned, and is the best-evidenced piece
of untested risk in the repo.

## If you are about to change this code

There is no claiming PR to wait for. If you are touching these handlers, **you are the opportunity**:
a test for the path you are changing is worth more here than anywhere else in the crate.

Note only that #494 will relocate the file, so a test added now moves with it.

## Verified by hand

2026-09-15: confirmed both functions and their line numbers, and confirmed #494's changeset
explicitly declines to test or decompose them. Not re-measured — the 2026-09-09 coverage run stands,
and nothing has edited the file since.
