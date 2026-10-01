# 2026-10-01 — A jail can name another session's conversation worktree over the host bridge

**Category:** Security — authorization gap, MEDIUM
**Source:** `#agent-worktree` stack, `2026-09-30-agent-worktree-isolated-edits` changeset (PR #560 validation)

The `ConversationWorktree` operation (`Pull` / `Remove`) has two ways in. Over the token path
(`ExecToolService`, authenticated by the session token) the session is the token's, so a foreign
conversation id can only name a directory inside the caller's own session. Over the **jail host
bridge** the session is whatever the request says:

- the runner rewrites the request to carry **its own** `session_id` from its environment
  (`tddy-sandbox-runner/src/conversation_root.rs::bind_to_this_session`) and strips the token;
- the host arm (`DaemonRpcHandler::handle_rpc` → `conversation_worktree_from_jail` in
  `tddy-session-lifecycle/src/connection_service/conversation_worktree_op.rs`) then trusts that
  `session_id`. Nothing host-side checks it against the session the jail was built for, so a
  process that reaches the bridge without going through the runner's rewrite can name any session id
  and have its conversation worktree pulled into, or removed from, that session's checkout.

The same arm resolves the session directory with `session_dir_for(&req.session_id)`, i.e.
`<tddy_data_dir>/sessions/<id>`, not through `sessions_base_for_user` as `resolve_exec_tool_worktree`
does for the token path — so a session of another OS user is looked up in the wrong tree.

## Fix

- Bind the session **host-side**: pass the jail's bound session id into `handle_rpc` (the per-jail
  handler already exists per sandbox) and reject a request whose `session_id` differs.
- Resolve the session worktree through the same path as `resolve_exec_tool_worktree`
  (`sessions_base_for_user`), not `tddy_data_dir/sessions/<id>`.
- Add a test beside `conversation_worktree_host_bridge_acceptance.rs`: a bridge request naming a
  different session than the handler's bound one is refused.

## Why it was deferred

The jail is trusted for exactly one session and the runner always rewrites the id, so exploiting
this needs code already running inside the jail that talks to the bridge directly. Binding the id
host-side means threading the bound session through `handle_rpc`'s signature, which every roster arm
on that bridge shares — a change to the bridge as a whole, outside this PR's boundaries
("no peer-clone changes", wiring only in the files #561–#563 also touch). Awaiting developer consent
for the deferral.

Related: [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md).
