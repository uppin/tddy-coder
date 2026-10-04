# 2026-10-04 — An agent restructures a session's worktree through tool calls

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Feature:
[Rust code restructuring](../rust-code-restructuring.md); also
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md).

When the daemon manages an index (`index_daemon:`), a session's agent can load, check, apply and inspect
restructure plans through six tool calls — `restructure_load`, `restructure_check`, `restructure_apply`,
`restructure_status`, `restructure_plans` and `restructure_anchors` — instead of shelling out to
`tddy-tools restructure`, which needed a socket no session had and no jail could reach.

- The index answers on the host, rooted at the session's own worktree; a plan or file path outside it is
  refused before the index is asked.
- A run answers one object — findings, one entry per applied operation, the outcome and a `refusal` —
  and a run the index refuses part-way keeps what it already applied. A stale operation is refused by
  its id.
- The tools are advertised only when the host can serve them: without an `index_daemon:` section none is
  registered and the session's tool set is unchanged.

Acceptance criteria: `restructure_check`, `restructure_apply` (with a stale operation refused by id) and
the host-side refusal of a path outside the worktree were each met by a passing test. **Deferred:** that
the tools reach the host from inside a jail, and that they are not advertised without `index_daemon:`,
is not covered by an end-to-end test; the gate and the host executor are tested separately. Backlog
entry `2026-10-04-session-restructure-tools-no-jail-end-to-end-test`.

Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../dev/changesets/2026-10-04-session-restructure-tools.md).
