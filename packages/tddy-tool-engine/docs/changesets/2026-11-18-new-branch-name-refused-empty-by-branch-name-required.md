# 2026-11-18 — `StartSession` says which agents refuse an empty `new_branch_name`

**Type:** Fix

The `new_branch_name` description in `start_session_args/schema.rs` states that an agent whose spec
declares `branch_name_required` (the built-in `claude-cli`) refuses an empty name under
`new_branch_from_base`, and that an empty `branch_worktree_intent` has a name generated for every
agent. It is pinned by `new_branch_name_says_an_agent_requiring_one_refuses_it_empty` in
`tests/start_session_schema.rs`. Full record:
[`docs/dev/changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md`](../../../../docs/dev/changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md).
