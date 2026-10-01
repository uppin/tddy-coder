# oversized-file: host_relay.rs

**Location:** `packages/tddy-sandbox-runner/src/host_relay.rs`
**Category:** oversized-file
**Detected:** 2026-10-01 — `/validate-changes` on PR #560 (`#agent-worktree` 1/4)
**Metrics:** **949 production lines** (no `#[cfg(test)]` module in the file; `wc -l`) · budget 500 · **1.9× over**
**Thresholds breached:** length 949 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-01 | 949 | first record of the *length*; 939 at `master`, +10 in PR #560 for carrying the conversation id through the host relay to `HostToolHandler::execute`. The file already carries a complexity record, [`complexity-host-relay-run-host-relay-inner.md`](complexity-host-relay-run-host-relay-inner.md) |

## What the tool found

`wc -l`; the file has no test module, so every line is production code. It is the host side of the
jail's relay: the allowlist, the request dispatch and the `run_host_relay_inner` loop that the
complexity record already measures.

## What would close it

Not designed. Splitting the dispatch of one request kind per module (tool call, session RPC,
conversation worktree) away from the relay loop is the likely seam, and it overlaps the loop
`complexity-host-relay-run-host-relay-inner.md` asks to decompose, so the two should be planned as
one change. Deferred in PR #560 — awaiting developer consent.
