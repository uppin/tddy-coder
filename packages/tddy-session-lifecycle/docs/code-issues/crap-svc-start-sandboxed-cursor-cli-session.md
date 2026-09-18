# crap: start_sandboxed_cursor_cli_session — untested, and the sweep ranked it low

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_start_sandboxed_cursor_cli_session.rs:40` — `start_sandboxed_cursor_cli_session`
**Category:** CRAP
**Detected:** 2026-09-18 by `tddy-tools analyze coverage` + `report` (245 tests across 96 files, join rate 51.9%)
**Metrics:** **CRAP 1,722** · complexity **41** · **never executed by any test** · rank **5/50** in this crate · 465 lines · nesting depth 3
**Restructure:** **no** — tests first. Same rule as [`crap-telegram-bot-handlers`](crap-telegram-bot-handlers.md)
**Status:** Open — **unclaimed**

## Measurement history

| Run | CRAP | Complexity | Coverage | Lines | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 1,722 | 41 | 0% | 465 | first detection |

## What the tool found

One of **three** sandboxed/plain session-start entry points in this crate, and the only one with no
test entering it at all. Because CRAP treats coverage as a boolean, the score is exactly
`cx² + cx` — the ranking among untested functions is complexity alone.

The three siblings measure very differently:

| Function | CRAP | Complexity | Covered | Lines | Nesting |
|---|---|---|---|---|---|
| `start_sandboxed_claude_cli_session` | 2,862 | 53 | **no** | 615 | 5 |
| **`start_sandboxed_cursor_cli_session`** | **1,722** | **41** | **no** | **465** | **3** |
| `start_session_core` | 80 | 80 | **yes, fully** | 842 | 6 |

## Why this record exists: the sweep missed it

**`/jev-restructuring` ranked this outside its top 100** and it therefore got no `complexity` record
in the batch. Jev's answers explain why, and they were not wrong:

| Jev signal | Value | Reading |
|---|---|---|
| `p_problem` | 0.83 | below the 0.91 floor the top 100 happened to cut at |
| `p_dispatch` | 0.06 | correctly: this is **not** a dispatcher |
| `category` | `unsure`, confidence **0.28** | the model flagged its own uncertainty |
| measured nesting | 3 | **under** the `/analyze-clean-code` threshold of 4 |

Jev was asked about **shape**, and the shape is genuinely flat: 465 lines of mostly straight-line
setup with 29 early exits and only 10 branch points. On the question it was asked, it answered
correctly, and its confidence of 0.28 said so.

What it cannot see is that this flat function has **complexity 41 and zero coverage**. That is the
CRAP axis, and no amount of reading the body semantically will produce it.

**This is the worked example of why both passes are mandatory.** The
[`jev-restructuring`](../../../../.agents/skills/jev-restructuring/SKILL.md) skill says to run
`/analyze-code-issues` alongside it and treat the disagreements as the finding. Here the
disagreement is the whole finding: the crate's **#5 risk** is invisible to a shape-based sweep, and
the function the sweep ranked **#1 in the entire repo** (`start_session_core`) turns out to be
**fully covered** and 46th of 50 by CRAP.

Ranking by shape alone would have put the safest of the three siblings first and left this one
unrecorded.

## Why it matters here

This is the entry point for starting a **sandboxed Cursor CLI session**. Nothing enters it in the
test suite, so every behaviour it has is unverified — and it sits beside two siblings that do
almost the same job, one of which is also untested.

A change to session startup usually has to be made in all three. Two of the three have no test to
catch a mistake in that edit.

## What would close it

**Tests before decomposition, and in that order.** Splitting an untested complexity-41 function
moves risk without reducing it.

1. A test that enters this path at all — CRAP falls from 1,722 to 41 the moment coverage is non-zero,
   because coverage is a boolean in the formula. That single test is the cheapest change on this
   list.
2. Only then consider whether the three startup paths should share extracted steps. At nesting 3
   this body is **not** tangled, so `extract_method` buys much less here than it does for its
   siblings — length is the only threshold it breaches.

`missing-tests` is never a delete: this is a live production entry point.

## If you are about to change this code

Unclaimed, so no in-flight PR conflict. **Check whether the same change is needed in
`svc_start_sandboxed_claude_cli_session.rs` and `svc_start_session_core.rs`** — the three paths have
no enforced relationship, and only the last of them has a test that would notice.
