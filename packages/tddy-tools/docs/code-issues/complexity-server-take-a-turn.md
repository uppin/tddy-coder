# complexity: take_a_turn

**Location:** `packages/tddy-tools/src/server.rs` — `take_a_turn`
**Category:** complexity
**Detected:** 2026-09-26 by `/analyze-clean-code` during `/pr-wrap` on PR #545
**Metrics:** **74 lines raw · 55 code lines · control nesting depth 3**
**Thresholds breached:** length 74 > 60
**Restructure:** `extract_method` — ordinary work
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Nesting | Note |
|---|---|---|---|---|
| 2026-09-26 | 74 | 55 | 3 | first detection, in PR #545 |

## What the tool found

Over the 60-line ceiling; nesting is fine. Inherited from `subagent_prompt_tool`, and, like its
namesake in `tddy-session-agents`, PR #545 is what made it a function shared by `subagent_prompt`
and `subagent_resume`.

## What would close it

The cancelled-conversation teardown (~27 lines) is self-contained and has nothing to do with
taking a turn: `retire_cancelled_conversation(&mut sessions, session_id, reason) -> Option<String>`
brings the host to ~47 lines.

## A naming note for whoever does this

There are now **two unrelated functions called `take_a_turn`**, this one and
`SessionAgentServiceImpl::take_a_turn` in `tddy-session-agents` — different crates, different
layers, different signatures. Defensible as the same concept at two altitudes, but it was a
coincidence rather than a decision, and it makes logs and stack traces ambiguous.
`dispatch_one_turn` here and `serve_one_turn` there would disambiguate.

## Verified by hand

2026-09-26 — Counted from the diff; confirmed the teardown block is contiguous and touches only
the session table.
