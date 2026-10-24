# A split start streams the codebase host's progress, with no deadline — PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: #start-progress 4/8 (base `feature/start-progress/supervisor-handoff`)

## Affected Features

- **Primary Feature**: [Remote-managed worktree](../remote-managed-worktree.md) — split sessions,
  § Failure is atomic
- **Related Feature**: [Cloning over SSH](../clone-ssh-passphrase.md) — forwarded-start budget and keep-alive

## Summary

When a session's codebase lives on another daemon, the start's progress on that daemon — its steps,
keep-alives, and (from node 5) clone progress — reaches the operator as it happens, through the
daemon running the agent. No timer ends the start: it ends with a result, an error from either host,
or the relay's idle timeout when a host goes silent. Every host sends a keep-alive at least every few
seconds for as long as a start is in progress, so a live start is never mistaken for a silent one.

## Proposed Changes

### What's Changing
- The split start and the agent-clone provision forward a **streamed** start to the codebase host and
  relay its events.
- The 330-second split-forward budget and the worktree-cut deadline on the session path are removed.
- A forwarded **unary** start also rides a stream internally, so it is bounded by peer silence, not a
  timer.
- Every streamed start emits keep-alives for its whole duration, not only while provisioning.
- A start refused or failed by the codebase host is reported as that host's answer; only a silent
  host is reported as silence (today a peer's `DEADLINE_EXCEEDED` is misreported as the forward's own).
- Split starts report their steps (worktree from the codebase host, agent from the agent host).

### What's Staying the Same
The caller-chosen workspace session id and the failed-start teardown; the 30 s relay idle timeout;
the whole-session forward (already streamed).

## Acceptance Criteria
- [ ] The codebase host's worktree step and keep-alives reach the caller of a split `StreamStartSession`.
- [ ] A codebase host that takes longer than the old 330 s budget, while sending keep-alives, still
      completes the start.
- [ ] A codebase host that goes silent ends the start with the relay's silence error, and the
      workspace session is torn down.
- [ ] A codebase host's own `DEADLINE_EXCEEDED` is reported as its answer, not as silence.
- [ ] A forwarded unary start completes past 330 s while the peer streams keep-alives.

## Out of Scope
Clone progress events (node 5); cancelling (node 8).
