# 2026-10-18 — `tddy-livekit-web`'s `startEchoServer` task intermittently exceeds its 60 s timeout

**Category:** Flaky test (CI), cause not yet found
**Source:** CI of [#645](https://github.com/uppin/tddy-coder/pull/645), then the investigation draft
[#651](https://github.com/uppin/tddy-coder/pull/651)

The `Web tests` job goes red with two failures and a lower test count:

```text
"before all" hook for "handles server streaming":
  `cy.task('startEchoServer')` timed out after waiting `60000ms`.
"before all" hook for "discovers EchoService via ServerReflectionInfo bidi stream":
  `cy.task('startEchoServerWithReflection')` timed out after waiting `60000ms`.
```

`reflection.cy.tsx` and `transport.cy.tsx` each fail in their `before all` hook, which skips their other eight tests
(2,859 tests run instead of 2,867). Each spec spends its full 60 s first, so the job also runs two minutes long.

## What is known

- **Not tied to one change.** Seen on `master` at `b73c97e0`, on `feature/satellite/coder-participant`, and on one
  attempt of #645. The same job passed on `master` at `ceec3dd2`, on #645's first run, and on all four attempts of the
  #651 hunt. Of the runs looked at, about one in three failed.
- **Not the image.** The failing runs on `master` and #645 pulled the same `livekit/livekit-server:master` digest as the
  passing ones (`sha256:5fa6c11e…`). This also is not the pull: a step already pulls it before the specs, and the pull
  finishes minutes earlier in the log.
- **Not reproducible locally.** `LivekitDockerTestkit.start()` completed in 240–380 ms, six of six.
- **A healthy start in CI is under a second**: `docker run` returned in 354–909 ms, the port mapped in about 25 ms,
  probe 1 failed fast (`fetch failed`, the container not yet listening), probe 2 answered 200, and the echo server
  printed `READY` about 0.2 s later.
- **The failure is a bare task timeout**, not one of the testkit's own errors (15 s for the port mapping, 20 s for
  readiness, 10 s for the echo server's `READY`). So the plugin process stalled rather than the container being slow.
  Two constructs in the testkit can do that; see
  [the testkit entry](2026-10-18-livekit-docker-testkit-can-stall-the-event-loop-with-no-deadline.md).

## What would close it

A failing sample with the testkit's phases in its log. `DEBUG=tddy:livekit-web:*` prints them (the repo's `debug` convention; `tddy:livekit-web:testkit` for the container,
`tddy:livekit-web:echo-server` for the task around it) (a timestamped line per
phase, each readiness probe at its start and at its answer, docker's own output); a `docker run` line with no
`returned` line is a hung docker, a `ready probe N … started` line with no answer is a hung `fetch`. It is enabled on
the `Web tests` component step by #651 and marked `FIXME(livekit-web-echo-server-hang)`. Until that lands on `master`
the next natural failure leaves no evidence, because the step discards docker's output and logs nothing.

**Why this is filed, not fixed:** the cause is a hypothesis between two candidates, and a fix chosen without the log
would leave a hang inside docker itself unexplained. History: the same symptom was once attributed to a slow image
pull, and the job gained a pre-pull step ("Pull the LiveKit image for tddy-livekit-web"); the failure outlived it.
