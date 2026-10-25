# oversized-file: host_relay.rs

**Location:** `packages/tddy-sandbox-runner/src/host_relay.rs`
**Category:** oversized-file
**Detected:** 2026-10-01 — `/validate-changes` on PR #560 (`#agent-worktree` 1/4)
**Metrics:** **1,033 production lines** (2026-10-25) (no `#[cfg(test)]` module in the file; `wc -l`) · budget 500 · **1.9× over**
**Thresholds breached:** length 1,033 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-01 | 949 | first record of the *length*; 939 at `master`, +10 in PR #560 for carrying the conversation id through the host relay to `HostToolHandler::execute`. The file already carries a complexity record, [`complexity-host-relay-run-host-relay-inner.md`](complexity-host-relay-run-host-relay-inner.md) |
| 2026-10-22 | 971 | **worse** by 22 in PR #670 (`#provider-route` 9/9): the relay reader holds a `WeakSender` (`send_if_open`, upgrades at the RPC and tunnel arms), a pre-existing channel-release defect from #481 fixed at the developer's request. Not split in #670 (scoped to files the PR pushed past 500) |
| 2026-10-25 | 1,033 | **worse** by 62 in PR #762 (`#term-snapshot` 3/4): `HostRelayConfig::terminal_events` / `with_terminal_events`, the `HostTerminalEvent` enum and the private `JailTerminalRoute` that delivers the jail's output and size changes in order. Split deferred with the developer's consent (code issues may be deferred in `#term-snapshot`) |

## What the tool found

`wc -l`; the file has no test module, so every line is production code. It is the host side of the
jail's relay: the allowlist, the request dispatch and the `run_host_relay_inner` loop that the
complexity record already measures.

## What would close it

Not designed. Splitting the dispatch of one request kind per module (tool call, session RPC,
conversation worktree) away from the relay loop is the likely seam, and it overlaps the loop
`complexity-host-relay-run-host-relay-inner.md` asks to decompose, so the two should be planned as
one change. Deferred in PR #560 — awaiting developer consent.
