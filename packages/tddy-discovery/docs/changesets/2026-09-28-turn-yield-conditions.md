# 2026-09-28 — Turn yield conditions: the predicates, the loop check, the stop reason

**Type:** Feature

`#subagent-control` 4/5, [#556](https://github.com/uppin/tddy-coder/pull/556). Cross-package entry:
`docs/dev/changesets/2026-09-28-turn-yield-conditions.md`.

A turn request may carry up to 8 **yield conditions** — each a tool name plus either an outcome
fact (equality against the call's result summary, node 1's vocabulary; the summary is absent
exactly when the dispatch produced nothing, which is what `error: true` fires on) or an argument
string field containing a bounded substring. `subagent/yield_condition.rs` holds the types, the
evaluation and the validation (unknown tool / a fact the tool's summary cannot carry / over-long
needle / too many conditions — refused before any model turn, naming the condition). `run_one_turn`
evaluates after each tool result's append and, on a fired condition, returns
`StopReason::YieldedToCaller` naming the condition and the yielded tool message's id — the result
stays in the transcript, the model never sees it, no further model turn is spent.

Code issues at wrap: `docs/code-issues/oversized-file-conversation.md` — first record, 512 lines
(+24, crossed here; decomposition deferred by developer consent past the stack);
`oversized-file-subagent.md` — 1,985 (+73, planned seams); `oversized-file-subagent-runtime.md` —
729 (+13). All open, unclaimed.
