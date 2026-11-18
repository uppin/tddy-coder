# 2026-11-18 — An idle session was reported as waiting on a permission; its terminal and activity reads came back empty

**Category:** Known defect (observed once; not reproduced)
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

The diagnostics session, idle at its prompt after a turn, was reported with a pending question of
`kind: permission` and an empty `request_id`. `SendSessionMessage` then refused it
(`waiting on permission ""`), and `AnswerSessionPermission` had no id to answer. The orchestrator got
through only by claiming the terminal and typing. In the same session `ReadSessionTerminal`
answered `has no live terminal` while it was `ExecutingTool`, and `ReadSessionActivity` from
cursor `0` returned no items although calls had completed.

**Why deferred:** seen once in another project's session; not reproduced, and the session status
code was not examined here.

**Partly fixed (2026-11-18, `#coder-start` 6/10, PR #946):** the permission-wait part is closed —
an idle prompt now records no pending question, a pending question always carries a request id, and
`SendSessionMessage` is delivered to an idle session. What remains is the two read oddities above.

**What would close it:** reproduce `ReadSessionTerminal` answering `has no live terminal` for an
`ExecutingTool` session, and `ReadSessionActivity` from cursor `0` returning nothing after completed
calls; fix whichever is real.
