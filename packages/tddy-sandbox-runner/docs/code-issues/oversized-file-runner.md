# oversized-file: runner.rs

**Location:** `packages/tddy-sandbox-runner/src/runner.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — `/pr-wrap` step 3.5 file-length gate on PR #545
**Metrics:** **2,742 production lines** (2026-10-25) (counted to the module-level `#[cfg(test)]`) · budget 500 ·
**5.2× over**
**Thresholds breached:** length 2,742 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 2,610 | first detection, at PR #545's merge-base |
| 2026-09-26 | 2,611 | +1 in PR #545 — **two prose comments only** ("the five …" → "the six …"), no code. Recorded because the gate attributes growth whatever its size, and because a one-line delta is the clearest possible evidence that this file's size predates that change |
| 2026-10-01 | 2,639 | **worse** by 28 over master's 2,611 in PR #560 (`#agent-worktree` 1/4): `call_tool(conversation_id, …)` and the `ConversationWorktree` relay arm. The jail-side root and session binding live in `conversation_root.rs` |
| 2026-10-22 | 2,698 | **worse** by 59 in PR #670 (`#provider-route` 9/9): the `--agent-kind opencode \| muse` dispatch arm (+20; the PTY spawn itself is in the new `harness_pty.rs`), and two pre-existing relay fixes — `acknowledge_attach` (+17) and `release_outbound` with its two call sites (+17) — plus `pub(crate)` on the PTY helpers `harness_pty.rs` reuses. Not split in #670: the orchestrator scoped decomposition to files the PR pushed past 500, and this one was 5.2× over before it |
| 2026-10-25 | 2,742 | **worse** by 44 in PR #762 (`#term-snapshot` 3/4): the terminal backlog carries `TerminalBacklogItem::{Output, Resized}`, `push_terminal_size`, and the generic and claude PTY threads report the size after `openpty` and after each accepted resize. Split deferred with the developer's consent (code issues may be deferred in `#term-snapshot`) |

## What the tool found

`wc -l` to the module-level `#[cfg(test)]`. Not measured: complexity, nesting, CRAP — this package
had no `docs/code-issues/` directory before this record, so it had never been analyzed.

Its sibling `host_relay.rs` already carries a complexity record
(`complexity-host-relay-run-host-relay-inner.md`, 263 lines / nesting 12), so the crate's two
largest files are now both on the books.

## Why it matters here

This is the in-jail runner: it holds the tool-IPC server, the relay allowlist consumption, the
egress shim, the secret-env resolution and the inner-agent spawn. It is the process every
sandboxed session's tools run inside, and it is security-relevant code — 2,611 lines is more than
a reviewer can hold while checking a confinement change.

## What would close it

Not yet designed. The plausible seams are the egress shim, the secret-env resolution and the
inner-agent spawn, each of which looks self-contained from the outside. Measure before planning:
this record exists to put the number on the table, not to prescribe a split.

**PR #545 grew it by one line of comment**, which is not the change that should pay for the
decomposition.

## Verified by hand

2026-09-26 — Confirmed the +1 is comment-only by reading the diff. Confirmed the count is to the
module-level `#[cfg(test)]`. No seam analysis performed.
