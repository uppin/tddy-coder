# 2026-11-21 — `CheckBaseFreshness` / `RefreshHome` are not forwarded to a peer host

**Category:** Known gap
**Source:** `#orchestra` 4/9 (PR #982), validation
**Consent:** the developer chose to defer this on 2026-11-22 at the `#orchestra` stack's TODO gate.

An orchestrator held on a host other than the one a call lands on is not served the base-freshness tools:
`run_base_freshness_tool` answers from the local sessions base only, and there is no `daemon_instance_id`
forwarding as the worktree and session tools have. The changeset's responsibility is the two tools and the
start ledger's base on the orchestrator's own host, so it was left out.

**What would close it:** route the two tools through the same peer forward the message tools use when the
caller's session record lives on another daemon, with an acceptance test over two in-memory daemons.
