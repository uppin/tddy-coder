# 2026-10-01 — `subagent_end` can race a prompt, report `ended: true` after a failed cancel, and leave an orphan

**Category:** Known gaps — deferred; developer consented 2026-10-01 (PR #560 `/pr-wrap`)
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`

Four edges of `subagent_end` / `subagent_cancel`
(`packages/tddy-tools/src/subagent_end.rs`, `packages/tddy-tools/src/server.rs::subagent_cancel_tool`, ~line 2065–2100):

1. **TOCTOU between the pending check and the retire.** `the_state_of` takes the sessions lock,
   checks no turn is pending, and releases it before the pull. A `subagent_prompt` can arrive in
   between; its turn then runs against a conversation that is pulled and retired under it.
2. **The cancel result is ignored.** `subagent_end_tool` calls `subagent_cancel_tool` and discards
   its answer, so `{"ended": true}` can be reported when the cancel failed.
3. **Cancel's worktree removal can race an in-flight call's `ensure`**, leaving a worktree and branch
   nothing addresses again (an orphan; see
   [`2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`](2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md)).
4. **A daemon-run conversation answers `pulled: null`**, which is indistinguishable from "this
   conversation never wrote" (see
   [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)).

Also: `subagent_end.rs` calls `server.rs::subagent_cancel_tool`, which calls back into
`subagent_end::discard_conversation_worktree` — a circular call between two modules.

**Why deferred:** the window needs a prompt racing an end on the same conversation, which the single
caller loop does not produce; closing it means changing how the sessions lock is held across the
whole close, in `server.rs` (over budget, shared with #561–#563).

**What closing it takes:** hold the sessions lock (or mark the conversation `closing`) from the
pending check through the retire; return the cancel's result and set `ended` from it; make the
removal wait for, or refuse, an in-flight `ensure`; give a daemon-run conversation an explicit answer
(`pulled: "not-applicable"` or an error) instead of `null`; move the shared close into one function
both tools call so the modules stop calling each other. After #563 lands.
