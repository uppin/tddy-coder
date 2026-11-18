# 2026-11-18 — An idle session waits for input, and a message to it is delivered

**Type:** Fix · `#coder-start` 6/10, PR [#946](https://github.com/uppin/tddy-coder/pull/946)

`SendSessionMessage`'s `refuse_while_waiting_on_an_answer` (`exec_tool/project_home/turn.rs`) refuses only a pending question with a `request_id`; a session waiting for input after its turn is delivered to, and an idle wake fires for it. Full record:
[`docs/dev/changesets/2026-11-18-idle-status.md`](../../../../docs/dev/changesets/2026-11-18-idle-status.md).
