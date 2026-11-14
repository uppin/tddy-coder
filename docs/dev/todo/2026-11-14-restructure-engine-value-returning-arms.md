# 2026-11-14 — `extract_method` refused when every match arm contains a value `return Ok(..)`

**Category:** Known limitation (engine capability)

**Source:** #901 (`#orchestrators` 12/12, the sweep) — item B8. Standing record:
`packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`.

## What happened

B8 re-measured on the sweep base: `start_session_on_request` 83 lines, `start_here_body` 273
lines (fn line to closing brace) in `packages/tddy-agent-launch/src/svc_start_session_core.rs` —
both over the 60-line ceiling. The sweep attempted the changeset's three cuts with
`restructure check --deep` against the warm index: the `NoAgent` arm, the placement/refusal
block, and the route `match` (`start_here_body` / `start_session_on_request`).

Each was refused with the same class: *this seam cannot be cut here: the range holds error guards
and a return of a value* — the route match on its `return Ok(Response::new(inner))`, the
`NoAgent` arm on its two `return Ok(started)`. Only a run whose every return is `return Err(..)`
can become a function called with `?`; these arms answer `Ok`.

Under green-yolo consent an engine refusal is not done by hand. The developer accepted B8 as
standing debt on 2026-11-14, reason: "restructure engine refused" — recorded on the record above.
Closing it today would need a hand restructure consented to outside green-yolo.

## Developer decision

**Deferred by the developer at the #orchestrators TODO gate, 2026-11-14** — the engine capability is not added in `#orchestrators`, and the seams it
blocks stay as recorded above until a later change closes it.

## What would close it

Engine work in `packages/tddy-code-restructuring` (and the rust-analyzer assist it drives):

- Convert a value `return Ok(..)` inside an extracted range to a tail expression of the extracted
  function (the caller then returns or propagates that value), instead of refusing the range.
- Pin the shape with a fixture whose match arms each end in a value `return Ok(..)`.

Then re-run B8: re-anchor the three cuts, prove with `restructure check --deep`, apply, and
delete or narrow
`packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`
with its final measurement in the change-history entry.
