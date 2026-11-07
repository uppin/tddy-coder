# 2026-11-07 — `tddy-client-web` holds helpers outside its charter

**Category:** Package boundary (placement)
**Source:** `#web-split` 6/17 (PR #843), `/validate-changes`

`tddy-client-web` is "what a component package may know about the outside world": generated
clients, connection types, ports and their doubles. PR #843 lowered helpers into it that both
`tddy-livekit-web` and `tddy-web` need, because it was the lowest package both already depend on.
Three of them are not about the outside world:

| Helper | Better home | Why |
|---|---|---|
| `src/lib/testId.ts` (`safeTestIdPart`) | `tddy-ui-web` | a rendering concern: how components and Cypress agree on `data-testid` |
| `src/lib/terminalFrameContinuity.ts` | the terminal package (`#web-split` terminal node) | terminal rendering state, not a wire |
| `src/lib/debugMask.ts` (+ `src/debug.d.ts`, `debug` dependency) | `tddy-react-commons`, or a small shared lib | process-wide diagnostics, no client or port in it |

`participantRole`, `terminalHistoryLoader`, `sessionAttachment`, the capability hooks and the host
directory **do** belong: each reads or describes a connection, a roster or an RPC.

**Why deferred:** successor nodes (#844 onwards) already import these from `tddy-client-web`; moving
them in #843 would turn every successor's rebase into a conflict on the same imports.

**What closing it takes:** move each row with `git mv`, re-export nothing from the old place, update
importers across the component packages and `tddy-web`, and keep each package's
`packageBoundary.test.ts` green. Natural owner: the `#web-split` sweep node, once every package
node has landed.
