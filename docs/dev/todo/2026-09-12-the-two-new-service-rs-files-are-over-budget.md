# 2026-09-12 — Both crates node 7 created open with an over-budget `service.rs`

**Category:** Future enhancement
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), changeset
[`2026-09-09-unbundle-session-agent-services`](../changesets/2026-09-09-unbundle-session-agent-services.md)

Measured on **production** lines (everything before the first `#[cfg(test)]`):

| File | Prod | Test |
|---|---:|---:|
| `tddy-session-agents/src/session_agent_clone.rs` | **1,157** | 98 |
| `tddy-session-agents/src/service.rs` | **876** | 0 |
| `tddy-session-activity/src/service.rs` | **819** | 0 |
| `tddy-session-agents/src/session_agent_roster.rs` | 394 | 270 |
| `tddy-session-agents/src/session_agent_status.rs` | 304 | 477 |

The plan's file-budget line named three files as over 500: `session_agent_clone.rs`,
`session_agent_status.rs` and `session_agent_roster.rs`. Measured on production lines, only the first
is — the other two are majority test module, `session_agent_status.rs` by more than half.

The two files that *are* over budget beside it are the two **new** `service.rs` files, which no plan
anticipated because neither existed when the budget was written. Each holds nine or eight RPC
handlers with their authorization order, refusals and framing. They are the shape of the extraction
rather than a file that grew.

## Why they were not split

A split for line count alone cuts cohesive units and puts churn on top of a move. Node 6 made the
same call for the same reason and recorded 10 files over 500.

But the honest version is narrower: the `service.rs` files have a plausible seam (the four roster
methods against the five conversation ones; the two hook reports against the four streams) and were
left whole because splitting them mid-stack cascades conflicts into nodes 8 and 9, which touch the
same crates' wiring. That reason expires when the stack lands.

## What closing it would take

Split each `service.rs` along the seam its own module header already names, after node 9. The
per-handler `Status` shapes and the shared helpers (`sessions_base_for_token`, the session-directory
resolution both crates repeat at the top of every method) are what has to stay in one place.
