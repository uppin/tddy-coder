# 2026-10-25 — No subagent tool discards a coding agent's conversation without applying its work

**Category:** Deferred from `#coagents` 9/9 (#776, sweep) — found at validation, needs a developer decision
**Source:** `packages/tddy-tools/src/server.rs` (`subagent_cancel_tool`); `packages/tddy-tools/src/subagent_end.rs`

Before #776, `subagent_cancel` closed every conversation and deleted its worktree, so it was how the
main agent threw a subagent's work away. #776 made `subagent_cancel` on a **coding agent** cancel only
the running turn and keep the conversation, its process and its worktree (the developer-approved
item 1). `subagent_end` still closes it, but it first applies the conversation's commits to the
caller's worktree. A detach closes it too, but a detach removes the agent from the session.

So the main agent has no way to end one coding-agent conversation and discard its work: cancelling
keeps it, ending applies it. The tool texts were corrected in #776 to say so; the capability is
missing.

## What would close it

A decision on the surface, then a test of it. Options: an argument on `subagent_cancel`
(`{ sessionId, close: true }`, sent as `keep_conversation: false`), an argument on `subagent_end`
that skips the pull, or a second `subagent_cancel` on an idle kept conversation closing it. Whichever
is chosen, a `tddy-tools` test shows the conversation closed on the daemon and its worktree deleted
with nothing applied to the caller's worktree.
