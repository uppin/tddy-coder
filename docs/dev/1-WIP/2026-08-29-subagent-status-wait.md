# Changeset: `subagent_status` waits, instead of being polled

**Date**: 2026-08-29
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md)
  - `server.rs` — `waitFor` / `timeoutMs` on `subagent_status`, the wait loop, the shared report
    serializer, and a `call_tool_by_name` arm
  - `session_agents/registry.rs` — a snapshot generation the wait parks on
    (`subscribe_to_snapshots`), published once per applied frame

## Related Feature Documentation

- [Session agent roster](../ft/daemon/session-agent-roster.md) — § `subagent_status` →
  *Waiting, rather than polling* (new), AC61-AC69 (new)

## Builds on

- `e45a4928` — added `subagent_status` itself, the web status display and jail-run reporting. This
  changeset adds only the wait; the report's shape, its `refusal`/`appliedRev` fields and the
  status vocabulary are that commit's and are unchanged here.

## Summary

`subagent_status` answers "can this agent be prompted?" at one instant. The main agent's real
question is "tell me when it can": an attach returns before the agent is usable, and a prompt sent
while its checkout is provisioning is refused naming the clone state (AC33). Without a wait the only
way to find out is to poll — and polling an MCP tool costs a whole model turn per look, which is the
most expensive way to wait that exists.

`subagent_status { agent, waitFor: "ready", timeoutMs }` parks until the named agent stops being
`connecting` and returns the same report a plain read returns, plus `timedOut`.

## Scope

- [x] **Tool**: `waitFor` / `timeoutMs`, the wait loop, the advertised schema
- [x] **Registry**: the snapshot generation a wait parks on
- [x] **Testing**: `subagent_status_wait_acceptance.rs` + three registry wake tests
- [ ] **Package Documentation**: `packages/tddy-tools/docs/` (wrap step)

## Technical Changes

### State A (current)

`subagent_status_tool` ignores its arguments and returns one read of `status_report()`. The registry
has no way to tell anyone a frame arrived: `apply_snapshot` mutates under a lock and returns.

### State B (target)

The tool takes three optional arguments. Without `waitFor` it behaves exactly as before — the same
JSON, byte for byte, with no `timedOut` key. With it, the call parks on a generation the registry
now publishes once per applied frame.

### Delta

#### `session_agents/registry.rs`

- `snapshots: watch::Sender<u64>` on `LiveAgentRoster`, and `subscribe_to_snapshots()`.
  A `watch` rather than a `Notify` because the receiver carries the version it last saw: a wait that
  reads the roster and *then* subscribes would sleep through the frame that landed between the two,
  and that frame is usually the one it was waiting for. Subscribing first and reading second is
  therefore safe, which is the only ordering a caller can get right.
- `apply_snapshot` is split around `replace_entries_with`, which does the work under the lock and
  reports whether the frame was applied. Two reasons, both structural rather than cosmetic: the
  generation is then published *after* the guard is dropped — waking a waiter while still holding
  the write lock only makes it block on the mutex it was just sent to read — and "was this frame
  applied?" becomes one answer across all three arms, so a frame too old to apply cannot wake a wait
  it changed nothing for. **No behaviour of the existing three arms changes.**

#### `server.rs`

- `readiness_is_settled` — the wait's predicate, deliberately *narrower* than promptability.
  Promptable is `status ∉ { CONNECTING, ERROR }`; only `CONNECTING` keeps the wait parked. A failed
  checkout is not something waiting fixes, so `ERROR` ends the wait and is reported — parking to the
  deadline would report the same failure later, and label it a timeout.
- `status_report_json` — the report serializer, lifted out of the tool body unchanged so the plain
  read and every wait ending produce identical JSON, plus `timedOut` when a wait was asked for. A
  field that is always `false` on a call that could not time out reads as a guarantee about a wait
  that never happened, so a plain read carries no `timedOut` at all.
- `subagent_status_tool` — argument parsing and dispatch. `agent` is the wait's **target, not a
  filter**: a plain read still reports every row even when one is named, so no existing caller can
  have its result narrowed by passing it.
- `wait_until_readiness_is_settled` — subscribe, then loop read → `select!(changed, sleep_until)`.
  Nothing here errors on the roster's account, which is the property `status_report` already holds:
  a detached agent is simply absent from `agents`, and a roster gone dark carries `refusal`. In both
  cases the report says what happened better than an error naming it would, and still carries every
  other row.
- `call_tool_by_name` gains a `subagent_status` arm, so the web Inspector's invoke button and the
  acceptance tests reach the handler by the path the agent uses. Safe there: a pure read of
  process-local state under a deadline it caps itself.

## Testing

### `packages/tddy-tools/tests/subagent_status_wait_acceptance.rs` (13)

Time is virtual (`start_paused`), so a deadline and the frame that beats it are ordered by the
runtime rather than by a sleep whose length a test would have to guess.

- wakes on a later frame, and on one at the revision already applied — the case a `rev`-driven wake
  would miss entirely
- already-promptable, `unknown`, and failed-checkout agents settle it without parking
- a detach settles it, and the report drops that row while keeping the others
- expiry returns `timedOut: true` with the current rows
- the cap holds: `timeoutMs` of ten minutes parks for exactly 120s, asserted on the runtime's
  virtual clock so it measures the deadline honoured rather than wall-clock patience
- `waitFor` without `agent`, an unrecognised `waitFor`, and a non-integer `timeoutMs` are refused
- a plain read carries no `timedOut`, and naming an `agent` without `waitFor` still reports everyone

### `session_agent_roster_client_acceptance.rs` (3 added)

The generation moves on a same-revision republish and on an attach, and does **not** move for a
frame too old to apply.

## Out of scope

- **`waitFor: "idle"`** — waiting for a running agent's turn to end. One line of predicate, and the
  wait machinery is already here, but nothing asks for it yet. Recorded under Future Enhancements
  with the one design question it raises that `"ready"` does not.
