# 2026-09-16 — `GrpcSessionTerminalResume` reconnects in `TAIL` under CI load

**Category:** Flaky test
**Source:** observed on PR #501 (`plan-red-i-want-to-make-a-poss`), CI run for `cc2e54a3`

`opens TAIL on first connect, then FROM_OFFSET with the tracked offset after a blip`
(`packages/tddy-web/cypress/component/GrpcSessionTerminalResume.cy.tsx:229`) failed one CI run in a
suite of **2,637** — 2,636 passed:

```
reconnect is FROM_OFFSET: expected 0 to equal 1
```

`StreamReplayMode.TAIL = 0` and `FROM_OFFSET = 1`
(`packages/tddy-web/src/gen/terminal_session_pb.ts:654`), so the reconnect opened in **TAIL**. The
preceding assertion on the same line — `opens.length > 1`, "a second open after restore" — passed,
so the reconnect did happen; it simply carried no tracked offset.

**Not caused by the change it appeared on.** PR #501 touches **zero** files under
`packages/tddy-web`, and the same check passed on that PR's own code commit (`4e9db380`) and on
`master`.

## Why it can lose the race

The test drives four steps in order (lines 231–244):

```
.deliverInitialTailFrame()   // pushes a frame carrying the tip offset
.simulateTransportBlip()     // clicks the button that nulls the daemon client
.restoreClient()             // clicks the button that returns a non-null client
.expectReconnectResumesFromOffset()
```

`deliverInitialTailFrame` enqueues the frame inside a `cy.then(...)`, which is synchronous, but the
component's `currentOffset` is React state — committed on a later tick. `simulateTransportBlip` is a
`.click()`, so ordinarily the commit lands in between. Under a loaded CI runner it need not: the
blip and restore can both run before the first frame's offset is in state, and a reconnect with no
tracked offset legitimately falls back to `TAIL`.

So the assertion depends on a state commit the test never waits for. The `0` is the production code
behaving correctly on the state it actually had.

## What closing it looks like

Wait for the first frame's offset to be **observable** before simulating the blip, rather than
assuming a click's worth of delay flushed it — assert a rendered consequence of that frame (the
driver already has `expectTerminalVisible`-style helpers to model one on), so the Given is
established before the When. Do not add a fixed wait: that re-tunes the same race rather than
removing it.

Worth checking the sibling cases in the same spec while there — any other one that delivers a frame
and immediately drives a transition has the same shape, whether or not it has failed yet.
