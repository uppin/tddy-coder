# oversized-file: service.rs

**Location:** `packages/tddy-session-agents/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — `/pr-wrap` step 3.5 file-length gate on PR #545
**Metrics:** **1,055 production lines** (counted to the module-level `#[cfg(test)]`) · budget 500 ·
**2.1× over**
**Thresholds breached:** length 1,055 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 877 | first detection, at PR #545's merge-base |
| 2026-09-26 | 1,055 | +178 in PR #545: `ResumeAgentConversation`, the shared `take_a_turn`, `TurnOnAConversation`, and the message-descriptor frame builders |

## What the tool found

`wc -l` to the module-level `#[cfg(test)]`, which is how the `oversized-file` category is defined.
Not measured: complexity, nesting, CRAP — the CRAP pipeline has never been run against this
package, and "not measured" is not "clean".

## Why it matters here

`SessionAgentServiceImpl` serves **ten** RPCs in one file — the roster four
(`Attach`/`Detach`/`List`/`Stream`), the conversation four
(`Open`/`Prompt`/`Resume`/`Cancel`) and the two state reports — each with its own peer-forwarding
branch, and now a shared turn path beneath two of them. The conversation half is the part that
moves when anything about turns changes: PR #545 touched it, and the roster half not at all.

## What would close it

The conversation half is the obvious seam: `open_agent_conversation`,
`prompt_agent_conversation`, `resume_agent_conversation`, `cancel_agent_conversation`,
`take_a_turn`, `TurnOnAConversation` and the frame builders form a contiguous concern with one
entry point per RPC. That is roughly 500 lines, leaving the roster and reporting halves near 550 —
so a second, smaller cut is needed to clear the budget outright.

Anchor with `tddy-tools restructure anchors`, never by hand; prove with `restructure check --deep`
against a warm index (`./run-index-daemon`).

**Deferred with developer consent in PR #545** — decomposing here would have added a mechanical
diff larger than the behaviour change it accompanied. See
`docs/dev/todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`.

## Verified by hand

2026-09-26 — Confirmed the count is to the module-level `#[cfg(test)]` and not an in-function
`#[cfg(test)]` branch (the naive reading under-reports; it scored a sibling file at 38 lines
instead of 2,488). Confirmed the ten RPCs and that the conversation four are contiguous. Did
**not** run `restructure check`, so the seam estimate is arithmetic, not a proven plan.
