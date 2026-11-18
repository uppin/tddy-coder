# 2026-11-18 — An idle session is reported as waiting on a permission with an empty request id

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

**What would close it:** reproduce it, then never emit a permission wait without a request id; report
an idle prompt as waiting for input.
