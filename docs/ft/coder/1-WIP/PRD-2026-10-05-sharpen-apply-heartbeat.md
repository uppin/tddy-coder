# A waiting restructure run says what it is waiting for - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement (diagnosability); explicitly **not** a change to how long a run may wait

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — § Waiting: the heartbeat,
  the stage in the "wait was cancelled" message, and what it still cannot tell.
- **Related surface**: `.agents/skills/code-restructuring/SKILL.md` (*Waiting* bullet) and the package
  README line "A run waits until the server is ready or until its caller stops waiting; there is no budget
  flag." — the rule stays, and gains one sentence.
- **Related Feature**: [Reusable LSP](../reusable-lsp.md) — unchanged in behaviour.

## Summary

A `restructure` run that waits for rust-analyzer can wait for many minutes, and by design nothing but the
caller ends the wait. Today, once the server stops sending new progress, the run says nothing at all.
This PRD makes every such wait print a heartbeat on a fixed 30-second cadence naming the stage it is in,
how long, which server it is waiting on, what that server last said, and for how long it has said nothing
new; and makes the cancel message name the stage. **It adds no deadline.**

## Background

A one-field rename `apply` did not return for over ten minutes after `check --deep` had answered. Its last
lines were `waiting for type inference at the anchor` and `working: build script num-bigint run`; the
daemon stayed up at 0% CPU. The cause was not found; the working theory is a build script that never
finished (the developer's endpoint protection was blocking freshly written scripts in the same window), so
rust-analyzer stayed non-quiescent for ever. A second theory, from reading the daemon: the first,
killed, client left its apply holding the root's queue, and the retry waited behind it — silently.

The developer decided (2026-10-05): **a heartbeat, no deadline.** The written design is that a run waits
until the server is ready or its caller stops it; a deadline would contradict it, and a timeout or a
pipeline around a run destroys the very signal that ends it. Two earlier todos asked the same question and
were deferred for the decision; this PRD answers it in the negative and keeps them open.

## Proposed Changes

### What's Changing

- Every wait the Rust backend makes for the index — the warm-up, type inference at an anchor, an assist's
  answer, an outline, a symbol — emits, every 30 seconds while it lasts, one line: `still waiting`, the
  elapsed time, the stage, which server (`rust-analyzer (pid N)` when the run started it itself, `behind a
  shared client` when it did not), whether the server says it is still loading, its last progress text and
  how long since it last said anything new.
- A request already sent to the shared language server is narrated while it waits, not only the polling
  between requests.
- In the index daemon, a run queued behind another operation on the same root says so, and for how long.
- A caller that has gone is noticed within one heartbeat (the next send fails), which releases a root a
  silent, abandoned run would otherwise hold.
- When the caller stops a wait, the message names the stage: "…had not finished indexing after 312s while
  waiting for type inference at src/lib.rs:41 (last progress: …) and the wait was cancelled."
- The `cargo check` that follows an apply is narrated the same way, with its pid *(Open decision D5; may be
  a follow-up)*.
- The test double `fake_lsp` can be busy for ever, so all of this is testable without a real server.

### What's Staying the Same

- **Nothing ends a wait sooner.** No deadline, no budget, no flag, no "give up after N beats".
- What "ready" means (quiescent and healthy) and every refusal class.
- Where output goes: lines travel through the run's `progress` sink; the library prints nothing, so no
  terminal UI and no RPC stream is touched. The command line shows them on stderr, stamped as before.
- The daemon's `warm` stream (it stays silent on a silent server; a recorded follow-up, because the
  function that narrates it is already too deeply nested to extend).
- rust-analyzer's own children (build scripts) stay invisible. The heartbeat quotes what rust-analyzer
  says it is doing; it does not and cannot identify a blocked script.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: a `wait` module (stage, clock, pure line formatter), five wait loops, the
  LSP bridge's request, the chatter's "unchanged for" clock, a `stage` on the incomplete-index refusal,
  an `Options` field and a registry constructor that carry the cadence.
- `tddy-lsp`: the test double only.
- `tddy-index-daemon`: the queue wait, a port carrying the cadence.
- `tddy-tools`: none; it already renders any line the daemon streams.
- Readiness state is read, never written, by the heartbeat; a test pins that a cancelled wait leaves the
  root not ready.

### User Impact

- A developer or agent waiting on a restructure run sees a line every 30 seconds saying what it waits on
  and whether the server is still doing anything. "Unchanged for 9m" against a last line naming a build
  script is the signature of the incident; "unchanged for 3s" is a slow load.
- Output grows by twelve to twenty lines for a six-to-ten-minute cold load.
- No new flags, no changed exit codes, no changed waiting behaviour.
- **Limits stated in the docs:** a stuck run still waits until stopped; a cold, self-started server is
  narrated between requests but not during one; the `warm` stream is not narrated.

## Implementation Plan

1. `fake_lsp` modes and the failing acceptance tests first (the draft contract), per the changeset.
2. The five polling waits, then the request in flight, then the cancel message.
3. The daemon: cadence port, the queue narration, the readiness-untouched test.
4. The compile gate (cuttable).
5. Docs, and re-measurement of the two index-daemon code-issue records.
6. Testing: integration against `fake_lsp` with a shortened cadence; unit tests on the line formatter and
   the chatter's clock; the existing "library prints nothing" guard stays green.

## Acceptance Criteria

- [ ] A wait on a server that stays busy prints a `still waiting` line at least every 30 seconds, naming
  the stage, the server, its last progress text and how long it has been unchanged ([Rust code restructuring](../rust-code-restructuring.md)).
- [ ] Each of the five waits names its own stage.
- [ ] A wait cancelled by its caller reports the stage it was in.
- [ ] A busy server is still waited on after any number of heartbeats: no deadline.
- [ ] A request in flight to the shared server is narrated while it waits.
- [ ] A run queued behind another on its root says so; a caller that hung up is noticed within one
  heartbeat and releases the root.
- [ ] A cancelled wait leaves the root not ready; the next warm waits for the graph.
- [ ] Nothing the library adds prints; the command line writes the heartbeat on stderr only.
- [ ] The docs state the limits: no deadline, rust-analyzer's children invisible, cold in-flight requests
  and the warm stream not narrated.
- [ ] Tests passing for the packages touched, run scoped; whole-workspace health read from CI.

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — § Waiting.
- [Reusable LSP](../reusable-lsp.md) — no behaviour change.

### Related Documentation

- Changeset: [2026-10-05-sharpen-apply-heartbeat.md](../../../dev/1-WIP/2026-10-05-sharpen-apply-heartbeat.md)
- Design rule it keeps: `packages/tddy-code-restructuring/README.md` line 20 and
  `packages/tddy-code-restructuring/docs/readiness-and-gates.md`.
- Todo that motivates it, **present only on branch `feature/carve/lifecycle-ports-agents` (PR #532)**:
  `docs/dev/todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md`.
- Code issue: `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md`.
