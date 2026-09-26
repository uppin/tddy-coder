# 2026-09-26 — A jail rebuild can re-run a tool call that already executed

**Category:** Defect — accepted risk
**Source:** `2026-09-26-subagent-turn-control-and-honest-tool-failure` changeset, PR #545 —
`/validate-prod-ready` finding 1. Developer decision on 2026-09-26: **ship at-least-once, record
it.**

`LocalExecTools::retry_in_a_rebuilt_jail`
(`packages/tddy-session-lifecycle/src/connection_service/local_exec_tools.rs`) retries **every**
tool name on a `ToolDispatchOutcome::TransportFailed` — `Write`, `StrReplace`, `Delete` and
`Shell` included. Three of the four failures that produce that outcome happen **after** the
request was written into the jail
(`packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`, `exchange_in_jail_tool_call`):

| Transport failure | Request sent? | Re-running it is |
|---|---|---|
| `its channel is closed` | no — a previous call killed the channel | safe |
| `the jail is no longer reading its session channel` | the send itself failed | safe |
| `it did not answer within 600s` | **yes, and the tool is probably still running** | at-least-once |
| `its session channel failed` / `… ended` | **yes, outcome unknown** | at-least-once |

On the last three the rebuild kills the runner mid-tool and executes the identical call again over
the same worktree. A `Write` can therefore land twice, or a `Shell` side effect can.

## Why it was accepted rather than narrowed

The obvious narrowing — retry only the two provably-not-executed errors — was proposed and
declined. It would still have covered the whole 2026-09-26 incident, since every call after the
first got `its channel is closed`.

Note for whoever revisits this: the reasoning that it is a rare edge case does **not** hold. The
default `Shell` budget is 30s, far under `IN_JAIL_TOOL_TIMEOUT` (600s), which makes the 600s path
look unreachable — but a **wedged runner is exactly the condition this whole feature exists for**,
and in the incident the call did sit until the 600s deadline. The unsafe path is the main path,
not an edge.

## What closing it would take

`exchange_in_jail_tool_call` already distinguishes the four cases internally; it collapses them
into one `TransportFailed(String)`. Carry the distinction in the type — `TransportFailed { reason,
request_reached_the_jail: bool }`, or two variants — and gate the retry on it. Roughly a
dozen lines, no new concepts, and `jail_relaunch_unit_tests.rs` already has the scaffolding to
test all four cases.

Gating on tool name instead (retry only non-mutating tools) is the weaker option: `Shell` is
mutating and is the tool most likely to be slow enough to hit the deadline.
