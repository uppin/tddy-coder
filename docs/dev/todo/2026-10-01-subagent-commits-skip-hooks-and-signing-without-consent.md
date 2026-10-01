# 2026-10-01 — Automatic subagent commits skip hooks and signing — developer consent NOT given

**Category:** Open decision — consent not given
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits` (PR #560 `/pr-wrap`)

Every git call `tddy-subagent-worktree` makes runs with `core.hooksPath=/dev/null` and
`commit.gpgsign=false`, and the per-call commit uses `--no-verify`
(`packages/tddy-subagent-worktree/src/git.rs` and the commit in `src/worktree.rs`, documented in
`packages/tddy-subagent-worktree/docs/conversation-worktree.md`). The implementer chose this: the
commits are automatic snapshots of a subagent's work, no developer hook (post-commit, post-checkout)
should run on the host on a subagent's behalf, and a signing prompt must not block a tool call.
A test pins it: `a_developers_post_commit_hook_does_not_run_for_a_subagents_commit`.

**The developer was asked and did not answer.** The deferrals of 2026-10-01 (file growth, host-bridge
binding, M6/M7) were consented to; this was not. Treat it as a decision taken by the implementer and
awaiting approval, not as approved.

**Cost of the choice:** a repository whose policy depends on a hook seeing every commit does not see
these; commits are unsigned.

**What would close it:** the developer decides — keep (record it as a decision in the feature doc),
or run hooks/signing for these commits (then drop the three overrides and the pinning test, and
accept that hooks run on the host for a subagent's work and a signing prompt can block a call).
