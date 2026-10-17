# 2026-10-18 — `livekitDockerTestkit.ts` can stall its Cypress plugin process with no deadline

**Category:** Defect (test infrastructure)
**Source:** investigating [the `startEchoServer` timeout](2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout.md)

`packages/tddy-livekit-web/cypress/support/livekitDockerTestkit.ts` runs inside the Cypress plugin process, where a
`cy.task` is abandoned after 60 s. It carries its own deadlines (15 s for `docker port`, 20 s for readiness), but two
of its constructs can defeat them:

- **`execSync` for every docker call** (`docker run`, each `docker port` attempt, `docker rm`). `execSync` blocks the
  whole Node event loop, so no timer fires while it waits: a `docker run` that hangs on a slow or wedged daemon is not
  interrupted by any of the testkit's deadlines, and the task simply times out.
- **`fetch()` in `_waitForReady` with no abort signal.** The 20 s deadline is checked only between iterations, so one
  request that connects and never answers stalls the loop for good.

Either explains a bare `cy.task … timed out after waiting 60000ms` where an ordinary slow start would have thrown a
named error. Neither is proven to be the cause, which is why this is not part of the diagnosis entry.

## What would close it

Run `docker run` asynchronously (`spawn`, with its own timeout and the container removed on expiry), and give the
readiness `fetch` an `AbortSignal.timeout`. Each failure then names itself within its own budget instead of burning
the task's 60 s. Both are small and low risk, and they are worth doing whether or not the log identifies the culprit,
since a hung dependency should fail as itself.

**Why deferred:** held until the failing sample says which construct it was; doing both blind removes the symptom
but not the knowledge.
