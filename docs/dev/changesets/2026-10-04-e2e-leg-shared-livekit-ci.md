# 2026-10-04 — The e2e leg runs against one pinned, shared LiveKit server

**Type:** Feature

`#e2e-leg` 4/5, PR [#581](https://github.com/uppin/tddy-coder/pull/581),
`feature/e2e-leg/shared-livekit-ci`, base `feature/e2e-leg/compile-timings`. Package entry:
[2026-10-04-livekit-image-pin.md](../../../packages/tddy-livekit-testkit/docs/changesets/2026-10-04-livekit-image-pin.md).
State B: [ci.md](../guides/ci.md) § The shared LiveKit server.

The `Rust e2e tests` leg starts one LiveKit server before nextest and removes it after, and every
LiveKit test reuses it through `LIVEKIT_TESTKIT_WS_URL` instead of launching a container each.

- `scripts/livekit-ci-server.sh start|stop|stop-after SECONDS`: `start` runs the pinned image with each of
  the three ports published on the same number inside and outside the container (as
  `LiveKitTestkit::start()` does, because ICE candidates embed container ports), waits for the Twirp API,
  fails loudly with the URL and container logs, and exports the URL through `$GITHUB_ENV`; `stop` is safe
  to run twice; `stop-after` is the drill's detached delayed removal. `scripts/livekit-ci-server.test.ts`
  pins them against a stub `docker` and `curl`.
- `ci.yml`: a start step and an `if: always()` stop step around nextest, e2e leg only; a
  `workflow_dispatch` input `kill_livekit_after_seconds` that removes the server mid-run.
- `.config/nextest.toml`: the `docker` override carries `slow-timeout = { period = "60s", terminate-after = 3 }`,
  sized from a green e2e run whose slowest single test took 39.8 s (run 37222869563).
- `./run-livekit-testkit-server` gained `--stop` and the same host=container port mapping.
- The image is pinned to `livekit/livekit-server:v1.13.7` in `.config/livekit-server.image`, one source
  for the script, the local script and the testkit.

The `docker` test-group stays serial; lifting it is node 5.

**Not proven here, read off CI later:** the pin on two consecutive green runs and one shuffled run; the
drill's result; the e2e job's `timeout-minutes`, left at the shared 150. They are recorded in
`docs/dev/todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`, narrowed rather than
resolved.

Backlog: [2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)
kept and narrowed; the two load-baseline flake entries it names are untouched.
